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
//! # The second axis — `w:formatting`, and why it was the rest of the feature
//!
//! Word's Restrict Editing pane is **two independent restrictions**, and only one of them is
//! `w:edit`. The other is `w:documentProtection/@w:formatting`, which Microsoft's SDK names
//! *"Only Allow Formatting With Unlocked Styles"*, whose companion is `w:style/@w:locked`
//! (ECMA-376 §17.7.4.6, *"Style Cannot Be Applied"*). `165` §7.6 measured all three of
//! `w:formatting`, `w:locked` and `w:latentStyles/@w:defLockedState` as imported, exported,
//! and **consumed by nothing** — the "modeled is not shipped" failure, three constructs deep,
//! inside the one feature family whose whole job is to refuse. `refuse_if_formatting_locked`
//! is their first consumer.
//!
//! It is a *separate* axis in the model and it has to be one here: a document may carry
//! `w:edit="none" w:formatting="1" w:enforcement="1"`, which is the pure formatting
//! restriction Word writes when an author ticks the formatting box and no editing box. Before
//! this, that document was freely reformattable here with no refusal and no finding.
//!
//! **The rule, stated once:** while a formatting restriction is enforced, an operation whose
//! effect is to change how existing content *looks* is refused — except installing a
//! paragraph style that is not locked, which is the one formatting change the restriction
//! exists to permit. Content edits are untouched, because the axes are independent.
//!
//! Three consequences are deliberate and none of them is obvious:
//!
//! 1. **Redefining a style is refused, locked or not.** Only a locked style's definition is
//!    normatively immutable, but a restriction that forbids setting one run bold while
//!    allowing `Normal`'s `w:rPr` to be rewritten has bought nothing: the second is strictly
//!    the more powerful of the two gestures. A rule a caller can walk around is a suggestion
//!    (`SKILL` §12), so the narrower sourced reading is not the one taken, and the locked case
//!    keeps its own more specific refusal. Numbering definitions are refused for the identical
//!    reason — a list's appearance is formatting held in a definition table.
//! 2. **`SetParagraphProperties` is judged by projection, not by variant**, the same way
//!    `comments` is and for the same reason: one operation carries both the style change the
//!    restriction permits and the direct paragraph formatting it forbids. Compare the
//!    properties with `style_ref` removed from both sides; equal means the operation moved the
//!    style and nothing else, and then the only remaining question is whether that style is
//!    locked.
//! 3. **`SetInlines` and `ReplaceTable` are allowed, and that is a residual this module
//!    names rather than hides.** Both are *inverse vehicles* — `SetInlines`'s own doc comment
//!    says so — and the inverse of an allowed content edit arrives as one. Undo runs through
//!    this choke point, so refusing them would refuse the undo of an edit the restriction
//!    permits; and a property-stripping projection cannot separate them from content, because
//!    an unformatted paste legitimately redraws run boundaries. So a caller that constructs a
//!    bare `SetInlines` by hand can still change formatting under a formatting restriction.
//!    That is the one hole in this axis, it is bounded by the fact that no editing gesture in
//!    this product produces one, and it is written down rather than left to be discovered.
//!
//! # What this axis is NOT
//!
//! It is not a security boundary, and nothing here should ever be described as one.
//! `w:documentProtection`'s password material is an integrity marker in plain-text XML:
//! ISO/IEC 29500-1 §17.15.1.29 says in its own note that the protection "does not encrypt the
//! document, and malicious applications might circumvent its use. This protection is not
//! intended as a security feature." No hash is modelled or verified here (ADR-052), and the
//! *grant* — [`crate::access`] — is the only thing in this engine that is a boundary.
//!
//! # Complexity
//!
//! O(the inlines the operation names) — never O(document). A protection check runs on the
//! keystroke path, so it may not walk the document (`107` §4 B1); the paragraphs are
//! resolved once, and only for the levels that need the projection. `readOnly` needs no
//! projection at all and is O(operations). The formatting axis adds one `BTreeMap` lookup per
//! style it has to grade and is behind a `bool` that is false in every unprotected document.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Document, DocumentProtectionEdit, InlineNode, ParagraphProperties, Revision, RevisionKind,
};

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
    /// `w:formatting="1"`: the document's formatting may not be changed directly. The
    /// **other axis** — it can refuse on a document whose `w:edit` restricts nothing, and
    /// it refuses a formatting gesture while leaving a content edit alone.
    FormattingRestricted,
    /// `w:formatting="1"` and the style the operation names carries `w:locked`
    /// (ECMA-376 §17.7.4.6, "Style Cannot Be Applied"). Separate from
    /// [`Self::FormattingRestricted`] because the reader's remedy differs: a locked style is
    /// one the author closed, and another style may still be applicable.
    StyleLocked,
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
            // Both sentences say what was refused and what is still possible, because that
            // is the half a reader can act on — and neither says "secure" or "locked by a
            // password", which `w:documentProtection` is not (ISO/IEC 29500-1 §17.15.1.29's
            // own note). "Its styles" rather than "a style" is the sourced behaviour: the
            // restriction permits the styles the document already defines and does not lock.
            Self::FormattingRestricted => crate::refused!(
                "document.protected-formatting",
                "This document is protected: its formatting can only be changed by applying \
                 one of its styles."
            ),
            Self::StyleLocked => crate::refused!(
                "document.protected-style-locked",
                "This document is protected and that style is locked, so it cannot be applied."
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
        // **The second axis, and it is checked AFTER the editing level, deliberately.** Both
        // can refuse the same operation on a document that carries both, and then the
        // editing level is the more useful sentence: "this document is read-only" tells a
        // reader their whole situation, where "formatting can only be changed by applying a
        // style" invites them to try a style that will also be refused. Narrowest authority
        // first is the same ordering rule `access_badge.mjs` follows in the chrome.
        if protection.formatting {
            refuse_if_formatting_locked(document, op)?;
        }
    }
    Ok(())
}

/// Refuses `op` when it changes formatting other than by applying an unlocked style.
///
/// The `w:formatting` axis, reached only from [`refuse_if_protected`] and only while a
/// protection is enforced *and* carries the flag. See this module's header for the rule and
/// for the three deliberate consequences; this function is where the operation set lives.
///
/// # Errors
///
/// [`ProtectionRefusal::StyleLocked`] when the operation names a style the document locked,
/// and [`ProtectionRefusal::FormattingRestricted`] for every other formatting change.
///
/// # Complexity
///
/// O(1) for every operation but [`Operation::SetParagraphProperties`], which resolves its
/// paragraph once — the same cost `comments` already pays — plus one `BTreeMap` lookup to
/// grade the style.
fn refuse_if_formatting_locked(
    document: &Document,
    op: &Operation,
) -> Result<(), ProtectionRefusal> {
    match op {
        // Direct character formatting. No style can authorise it, so there is nothing to
        // project and nothing to grade.
        Operation::FormatText { .. } | Operation::ClearFormatting { .. } => {
            Err(ProtectionRefusal::FormattingRestricted)
        }
        // The one operation that carries both halves. See the header's point 2.
        Operation::SetParagraphProperties { node, properties } => {
            let Some(current) = current_properties(document, *node) else {
                // A write this check cannot place is refused rather than waved through —
                // the rule `is_comment_only` and the facade's forms check both follow.
                return Err(ProtectionRefusal::FormattingRestricted);
            };
            if without_style(current) != without_style(properties) {
                return Err(ProtectionRefusal::FormattingRestricted);
            }
            // Only the style moved. It may be installed if the document did not lock it —
            // and REMOVING a style (`None`) is not applying one, so it is allowed: it
            // restores the document defaults rather than imposing the author's formatting
            // on content they locked away from.
            match properties.style_ref {
                Some(style) if document.definitions().style_locked(style) => {
                    Err(ProtectionRefusal::StyleLocked)
                }
                _ => Ok(()),
            }
        }
        // Formatting held in a definition table, which is the more powerful gesture rather
        // than the lesser one. Header point 1.
        Operation::SetStyleDefinition { id, .. } => {
            if document.definitions().style_locked(*id) {
                Err(ProtectionRefusal::StyleLocked)
            } else {
                Err(ProtectionRefusal::FormattingRestricted)
            }
        }
        Operation::SetAbstractNumbering { .. }
        | Operation::SetNumberingInstance { .. }
        // Direct table and drawing formatting. `ReplaceTable` is NOT here: header point 3.
        | Operation::SetTableProperties { .. }
        | Operation::SetTableCellProperties { .. }
        | Operation::SetShapeFill { .. }
        | Operation::SetShapeStroke { .. } => Err(ProtectionRefusal::FormattingRestricted),
        // **Exhaustive on purpose, with no `_` arm**, for this module's standing reason: the
        // 61st operation must be a compile error here rather than a formatting change that
        // arrives exempt. Every arm below is content, structure, page geometry or metadata —
        // none of them is a "formatting modification" in the sense ECMA-376 §17.15.1.30
        // restricts, and refusing one would refuse an edit Word permits, which is a defect
        // in the other direction and a worse one on a file this engine merely read.
        //
        // The two inverse vehicles — `SetInlines` and `ReplaceTable` — are the residual, and
        // they are here rather than above because undo runs through this choke point. Header
        // point 3 carries the argument and names the hole.
        //
        // Page and section setup is deliberately not formatting either: Word's own route to
        // the style whitelist is Manage Styles, the pane's axis is character and paragraph
        // formatting, and nothing sourced says a protected document's margins are frozen.
        // Recorded as a decision rather than left ambiguous — if it turns out Word freezes
        // them, the arms move up and the guard for the move already has a home.
        Operation::InsertText { .. }
        | Operation::DeleteText { .. }
        | Operation::SplitParagraph { .. }
        | Operation::JoinParagraphs { .. }
        | Operation::SetHyperlink { .. }
        | Operation::SetInlines { .. }
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
        | Operation::ReplaceTable { .. }
        | Operation::SetCoreProperties { .. }
        | Operation::UpdateReviewState { .. }
        | Operation::SetSectionGeometry { .. }
        | Operation::SpliceSectionBoundary { .. }
        | Operation::SetMediaReference { .. }
        | Operation::SetChartDefinition { .. }
        // Never reached: `refuse_if_protected` exempts this operation before either axis is
        // consulted, for the one-way-door reason `exempt_from_protection` records. `Ok` is
        // nonetheless the right answer to put here rather than a refusal, because a
        // formatting restriction has no standing to stop itself being lifted.
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
        | Operation::SetTextBoxBody { .. }
        // A lock is a restriction on later edits, not formatting of the object, like
        // its alt text beside it in the same non-visual properties.
        | Operation::SetObjectLocks { .. }
        // A document setting, as `SetEvenAndOddHeaders` is.
        | Operation::SetTrackRevisions { .. }
        // Page and section setup, by the decision recorded above.
        | Operation::SetSectionFormProtection { .. } => Ok(()),
    }
}

/// The paragraph's current properties, or `None` when the operation names a paragraph that
/// is not there.
///
/// Resolved through the same walk [`current_inlines`] uses, so a `SetParagraphProperties`
/// costs what an `UpdateReviewState` already costs under `comments`.
fn current_properties(document: &Document, node: NodeId) -> Option<&ParagraphProperties> {
    crate::find_paragraph_any(document, node).map(|paragraph| &*paragraph.properties)
}

/// `properties` with `w:pStyle` removed — the projection the formatting axis compares.
///
/// Two paragraph property sets have the same projection exactly when they differ in nothing
/// but which style is applied, which is the one formatting change `w:formatting` permits.
fn without_style(properties: &ParagraphProperties) -> ParagraphProperties {
    let mut projected = properties.clone();
    projected.style_ref = None;
    projected
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
        | Operation::SetChartDefinition { .. }
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
        | Operation::SetTextBoxBody { .. }
        | Operation::SetObjectLocks { .. }
        // Not a comment. (Under `trackedChanges`, `is_tracked_only` admits turning
        // tracking ON before it gets here.)
        | Operation::SetTrackRevisions { .. }
        | Operation::SetSectionFormProtection { .. } => false,
    }
}

/// Whether `op` only adds tracked marks — never accepts or rejects one, and never edits
/// untracked.
///
/// Word's tracked-changes restriction allows comments too, so this is a superset of
/// [`is_comment_only`]: the projection below drops comment markers as well.
pub(crate) fn is_tracked_only(document: &Document, op: &Operation) -> bool {
    match op {
        // ECMA-376 §17.15.1.29: `trackedChanges` "shall imply the presence of the
        // `trackRevisions` element, and applications shall not allow that element's
        // state to be changed to false". Turning tracking ON is what the restriction
        // asks for; turning it off is refused.
        Operation::SetTrackRevisions { enabled } => *enabled,
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

/// `inlines` as they stand if every tracked change in them is **accepted**, with comment
/// markers dropped.
///
/// The mirror of [`before_projection`], and the second half of what makes a review decision
/// exactly testable. An insertion contributes its content unwrapped, because accepting it makes
/// the text plain; a deletion contributes nothing, because accepting it removes the text. So
/// two inline lists have the same after-projection exactly when they describe the same
/// *resolved* text, however the tracked marks over it differ.
///
/// # Why both projections are needed and neither alone is enough
///
/// Accepting a tracked change leaves the after-state untouched and moves the before-state;
/// rejecting one leaves the before-state untouched and moves the after-state. One projection
/// therefore recognises one direction of decision and mistakes the other for an edit. Together
/// they recognise both, and an edit smuggled alongside either moves **both**, which is what
/// [`is_review_only`] relies on.
fn after_projection(inlines: &[InlineNode]) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match inline {
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_) => {}
            InlineNode::Revision(revision) => match revision.kind {
                // Accepting an insertion makes its content plain: contribute it unwrapped, so
                // the insertion and the already-accepted text project alike.
                RevisionKind::Insertion | RevisionKind::MoveTo => {
                    out.extend(after_projection(&revision.inlines));
                }
                // Accepting a deletion removes the content, so it contributes nothing.
                RevisionKind::Deletion | RevisionKind::MoveFrom => {}
            },
            other => out.push(map_children(other, after_projection)),
        }
    }
    out
}

/// Whether `op` only **resolves** tracked changes — `143` §10's `review` class, exactly.
///
/// # Why this is exact, where the module's notes once said it could not be
///
/// `access`'s own documentation recorded that no exact rule separated a reviewer from an
/// editor, because [`is_tracked_only`] is a *negative* test that accept, reject and untracked
/// typing all fail alike. That was a true statement about `is_tracked_only` and a false
/// conclusion about the question. The positive rule needs two facts, and both are already here:
///
/// 1. **A revision disappeared, and none appeared.** [`revision_ids`] gives this exactly.
///    Nothing but an accept or a reject removes a revision from a paragraph through this
///    operation, and a reviewer authors no suggestions of their own.
/// 2. **The decision explains the whole new state.** Accepting leaves
///    [`after_projection`] untouched; rejecting leaves [`before_projection`] untouched; and an
///    untracked edit carried in alongside either moves **both**, because every projection
///    retains all plain content. So *one of the two projections is unchanged* is the exact
///    test, with no heuristic and no third projection invented.
///
/// # What it deliberately refuses, named rather than hidden
///
/// A single paragraph that **accepts one revision and rejects another in one operation**
/// satisfies neither projection and is refused. That is a real narrowing and it is stated
/// rather than papered over: no gesture in Word or Docs produces it — accept and reject are
/// per-change or per-selection commands, and "accept all"/"reject all" are each one direction —
/// so the refusal is reachable only by a client that batched two opposite decisions into one
/// paragraph's rewrite, which it can split. Two *different* paragraphs deciding in opposite
/// directions in one operation is fine, because the test is per paragraph.
///
/// # Complexity
///
/// O(the inlines of the paragraphs named), the same cost [`is_tracked_only`] already pays and
/// for the same reason: it reuses projections rather than walking the document.
pub(crate) fn is_review_only(document: &Document, op: &Operation) -> bool {
    let Operation::UpdateReviewState { paragraphs, .. } = op else {
        return false;
    };
    let mut resolved_any = false;
    for state in paragraphs {
        let Some(current) = current_inlines(document, state.node) else {
            // A write this check cannot place is refused rather than waved through — the rule
            // `current_inlines` documents.
            return false;
        };
        let held = revision_ids(current);
        let proposed = revision_ids(&state.inlines);
        // A reviewer decides on other people's suggestions; it never authors one. An id the
        // paragraph did not already hold is a new tracked change, which is `suggest`.
        if proposed.iter().any(|id| !held.contains(id)) {
            return false;
        }
        let accepted_only = after_projection(current) == after_projection(&state.inlines);
        let rejected_only = before_projection(current) == before_projection(&state.inlines);
        if !accepted_only && !rejected_only {
            return false;
        }
        resolved_any |= held.iter().any(|id| !proposed.contains(id));
    }
    // An operation that resolves nothing is not a review decision. Without this an untouched
    // paragraph list would satisfy every clause above and `review` would admit a no-op that a
    // commenter's own class should answer — and, worse, a comment-marker-only change, which is
    // `comment`'s business and must not be reachable through this class instead.
    resolved_any
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

    // Separate `use` line (kept out of the sorted block above) so a parallel lane adding a
    // v1 import here does not conflict in the shared sorted list.
    use casual_doc_model::v1::{LatentStyles, StyleId};

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
            password: None,
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

    /// **The same operation is an accept on one document and a reject on another.**
    ///
    /// The guard `is_review_only` was owed and the one that makes the predicate's claim
    /// falsifiable. `UpdateReviewState` carries the *resulting* inlines, so an operation on its
    /// own has no direction at all: `[run 11, run 31]` is an accepted insertion against one
    /// document and a rejected deletion against another, and the two differ only in what the
    /// paragraph already held. A guard that asserted one direction on one fixture would be
    /// satisfied by a predicate keyed on operation *shape* — which is precisely the mistake
    /// `is_tracked_only` made when it was read as answering this question.
    ///
    /// Four cells: two operations × two documents, one per (direction, revision kind) pair.
    /// Each cell also asserts **which half recognised it**, because that is the observable
    /// consequence of the two projections doing different work — the same operation is caught
    /// by `after_projection` on one document and by `before_projection` on the other.
    #[test]
    fn one_operation_is_an_accept_on_one_document_and_a_reject_on_another() {
        // The same inner run id in both, so the two documents differ in the revision KIND and
        // in nothing else. That is what lets one operation be submitted against both.
        let holds_an_insertion = vec![
            run(11, "abc"),
            revision(30, RevisionKind::Insertion, vec![run(31, "xyz")]),
        ];
        let holds_a_deletion = vec![
            run(11, "abc"),
            revision(40, RevisionKind::Deletion, vec![run(31, "xyz")]),
        ];
        // The revision gone and its text gone with it.
        let decided_away = vec![run(11, "abc")];
        // The revision gone and its text left standing as plain content.
        let decided_plain = vec![run(11, "abc"), run(31, "xyz")];

        // `(after-projection preserved, before-projection preserved)` — exactly one of them,
        // per cell, and which one is the direction of the decision.
        const ACCEPTED: (bool, bool) = (true, false);
        const REJECTED: (bool, bool) = (false, true);

        for (what, held, proposed, direction) in [
            (
                "rejecting an insertion",
                &holds_an_insertion,
                &decided_away,
                REJECTED,
            ),
            (
                "accepting a deletion",
                &holds_a_deletion,
                &decided_away,
                ACCEPTED,
            ),
            (
                "accepting an insertion",
                &holds_an_insertion,
                &decided_plain,
                ACCEPTED,
            ),
            (
                "rejecting a deletion",
                &holds_a_deletion,
                &decided_plain,
                REJECTED,
            ),
        ] {
            let document = holding(held.clone());
            let op = review(proposed.clone());
            assert!(
                is_review_only(&document, &op),
                "{what} is not a review decision"
            );
            assert_eq!(
                (
                    after_projection(held) == after_projection(proposed),
                    before_projection(held) == before_projection(proposed),
                ),
                direction,
                "{what} was recognised by the wrong half: the two projections are what make \
                 one operation mean opposite things on two documents, so a rule keyed on the \
                 operation's shape would answer both cells the same way"
            );
        }

        // THE SENSITIVITY A READER WILL WONDER ABOUT, named here rather than found later. The
        // projections compare inlines by **full equality, `NodeId` included**, so accepting an
        // insertion must hand back the same run and not an equal-looking new one: a client that
        // re-mints the id while unwrapping the revision is refused.
        let reminted = review(vec![run(11, "abc"), run(999, "xyz")]);
        assert!(
            !is_review_only(&holding(holds_an_insertion.clone()), &reminted),
            "a decision that re-minted the accepted run's id was admitted, so the identity the \
             rest of the engine anchors comments, carets and revisions to is not preserved \
             across an accept"
        );

        // And that is a CONVENTION this module has kept since ADR-052 rather than something new
        // `is_review_only` introduced — asserted, because a reader has every reason to check.
        // `is_tracked_only` compares the same way: a tracked deletion that re-wrapped the text
        // under a fresh run id is not the same before-state.
        let plain = holding(vec![run(11, "abc")]);
        assert!(
            is_tracked_only(
                &plain,
                &review(vec![revision(
                    40,
                    RevisionKind::Deletion,
                    vec![run(11, "abc")]
                )])
            ),
            "a tracked deletion that keeps the run's identity is a suggestion"
        );
        assert!(
            !is_tracked_only(
                &plain,
                &review(vec![revision(
                    40,
                    RevisionKind::Deletion,
                    vec![run(999, "abc")]
                )])
            ),
            "`is_tracked_only` has compared ids since ADR-052, so `is_review_only` doing the \
             same is one convention rather than two"
        );
    }

    /// An untracked edit smuggled beside a decision is refused, **and the bare decision is
    /// not**.
    ///
    /// Both halves, because a predicate that refused everything would satisfy the first one on
    /// its own. This is the property the whole design rests on: every projection retains all
    /// plain content, so an edit carried in alongside a decision moves *both* projections and
    /// no clause can explain the result.
    #[test]
    fn an_edit_smuggled_beside_a_decision_is_refused_and_the_bare_decision_is_admitted() {
        let held = vec![
            run(11, "abc"),
            revision(30, RevisionKind::Insertion, vec![run(31, "xyz")]),
        ];
        let document = holding(held);

        // The two bare decisions, asserted admitted FIRST so the refusals below are known to be
        // about the smuggling and not about the fixture.
        for (what, op) in [
            ("accepting it", review(vec![run(11, "abc"), run(31, "xyz")])),
            ("rejecting it", review(vec![run(11, "abc")])),
        ] {
            assert!(
                is_review_only(&document, &op),
                "{what} must be admitted, or this guard cannot tell a smuggled edit from a \
                 decision"
            );
        }

        for (what, op) in [
            (
                "a character typed into the plain text beside an accept",
                review(vec![run(11, "abcQ"), run(31, "xyz")]),
            ),
            (
                "a character typed into the plain text beside a reject",
                review(vec![run(11, "abcQ")]),
            ),
            (
                "a brand-new untracked run beside an accept",
                review(vec![run(11, "abc"), run(31, "xyz"), run(50, "!")]),
            ),
            (
                "a character typed into the text being accepted",
                review(vec![run(11, "abc"), run(31, "xyzQ")]),
            ),
        ] {
            assert!(
                !is_review_only(&document, &op),
                "{what} was admitted as a review decision: an untracked edit must move BOTH \
                 projections, which is the only reason this class is exact"
            );
        }
    }

    /// A reviewer authors nothing and a suggester decides nothing — in both directions.
    ///
    /// The two classes are **disjoint**, not rungs on one ladder, and that is why `review` is
    /// its own clause in `refuse_if_not_permitted` rather than an ordering of the others. Each
    /// gesture is asserted against *both* predicates, so a change that quietly widened either
    /// one is caught by the half that should have said no.
    #[test]
    fn a_reviewer_authors_nothing_and_a_suggester_decides_nothing() {
        let plain = holding(vec![run(11, "abcdefgh")]);
        for (what, op) in [
            (
                "a proposed insertion",
                review(vec![
                    revision(30, RevisionKind::Insertion, vec![run(31, "x")]),
                    run(11, "abcdefgh"),
                ]),
            ),
            (
                "a proposed deletion",
                review(vec![revision(
                    40,
                    RevisionKind::Deletion,
                    vec![run(11, "abcdefgh")],
                )]),
            ),
        ] {
            assert!(
                is_tracked_only(&plain, &op),
                "{what} must be a suggestion, or this guard measures the wrong thing"
            );
            assert!(
                !is_review_only(&plain, &op),
                "{what} reached `review`: a reviewer resolves what it did not author, so a \
                 preset that grants `review` and not `suggest` would be handing over the wider \
                 right by the back door"
            );
        }

        let holds_an_insertion = vec![
            run(11, "abc"),
            revision(30, RevisionKind::Insertion, vec![run(31, "xyz")]),
        ];
        let holds_a_deletion = vec![
            run(11, "abc"),
            revision(40, RevisionKind::Deletion, vec![run(31, "xyz")]),
        ];
        for (what, held, proposed) in [
            (
                "accepting an insertion",
                &holds_an_insertion,
                vec![run(11, "abc"), run(31, "xyz")],
            ),
            (
                "rejecting an insertion",
                &holds_an_insertion,
                vec![run(11, "abc")],
            ),
            (
                "accepting a deletion",
                &holds_a_deletion,
                vec![run(11, "abc")],
            ),
            (
                "rejecting a deletion",
                &holds_a_deletion,
                vec![run(11, "abc"), run(31, "xyz")],
            ),
        ] {
            let document = holding(held.clone());
            let op = review(proposed);
            assert!(is_review_only(&document, &op), "{what} is not a decision");
            assert!(
                !is_tracked_only(&document, &op),
                "{what} reached `suggest`: the tracked-changes class must not be able to \
                 resolve somebody else's suggestion, which is the whole reason `review` had to \
                 become a positive rule"
            );
        }

        // The cases that charge the *new revision id* clause specifically, and the reason it is
        // not redundant with the projections. `before_projection` is **blind to both revision
        // kinds** — an insertion contributes nothing and a deletion contributes its content
        // unwrapped — so authoring a brand-new tracked mark over text that is already there
        // leaves the before-projection untouched. Rejecting somebody else's suggestion in the
        // same operation then supplies `resolved_any`, and every clause but the id check is
        // satisfied by an operation that authored a suggestion.
        let document = holding(holds_an_insertion);
        for (what, proposed) in [
            (
                "rejecting a suggestion while authoring a new tracked insertion",
                vec![
                    run(11, "abc"),
                    revision(50, RevisionKind::Insertion, vec![run(51, "!")]),
                ],
            ),
            (
                "rejecting a suggestion while authoring a new tracked deletion",
                vec![revision(50, RevisionKind::Deletion, vec![run(11, "abc")])],
            ),
        ] {
            assert_eq!(
                before_projection(&[
                    run(11, "abc"),
                    revision(30, RevisionKind::Insertion, vec![run(31, "xyz")]),
                ]),
                before_projection(&proposed),
                "{what} must leave the before-projection untouched, or this case does not reach \
                 the id clause at all"
            );
            assert!(
                !is_review_only(&document, &review(proposed)),
                "{what} was admitted: a reviewer authored a suggestion of their own, which the \
                 projections cannot see and only the vanished-and-appeared id ledger can"
            );
        }
    }

    /// Opposite decisions in **one** paragraph are refused and across **two** are not.
    ///
    /// The narrowing `is_review_only` documents, asserted in both directions so that it is a
    /// stated limit rather than a hidden one. Each decision alone is asserted admitted first:
    /// without that the refusal could be a predicate that cannot read this fixture at all.
    #[test]
    fn opposite_decisions_collide_in_one_paragraph_and_do_not_across_two() {
        let holds_both = vec![
            revision(30, RevisionKind::Insertion, vec![run(31, "ins")]),
            run(11, "abc"),
            revision(40, RevisionKind::Deletion, vec![run(41, "del")]),
        ];
        let document = holding(holds_both);

        let accept_the_insertion = review(vec![
            run(31, "ins"),
            run(11, "abc"),
            revision(40, RevisionKind::Deletion, vec![run(41, "del")]),
        ]);
        let reject_the_deletion = review(vec![
            revision(30, RevisionKind::Insertion, vec![run(31, "ins")]),
            run(11, "abc"),
            run(41, "del"),
        ]);
        assert!(
            is_review_only(&document, &accept_the_insertion),
            "accepting one of two revisions and leaving the other alone must be admitted"
        );
        assert!(
            is_review_only(&document, &reject_the_deletion),
            "rejecting one of two revisions and leaving the other alone must be admitted"
        );

        let collided = review(vec![run(31, "ins"), run(11, "abc"), run(41, "del")]);
        assert!(
            !is_review_only(&document, &collided),
            "one paragraph accepting one revision and rejecting another in a single operation \
             satisfies neither projection, and the refusal is the documented narrowing: no Word \
             or Docs gesture produces it, and a client that batched two opposite decisions into \
             one paragraph's rewrite can split them"
        );

        // The SAME two decisions, one paragraph each: admitted, because the test is per
        // paragraph. This is the half that makes the narrowing narrow rather than a ban on
        // deciding in both directions at once.
        let split = holding_two(
            vec![
                revision(30, RevisionKind::Insertion, vec![run(31, "ins")]),
                run(11, "abc"),
            ],
            vec![
                run(12, "def"),
                revision(40, RevisionKind::Deletion, vec![run(41, "del")]),
            ],
        );
        let across = Operation::UpdateReviewState {
            paragraphs: vec![
                ReviewParagraphState {
                    node: n(10),
                    inlines: vec![run(31, "ins"), run(11, "abc")],
                },
                ReviewParagraphState {
                    node: n(20),
                    inlines: vec![run(12, "def"), run(41, "del")],
                },
            ],
            comments: None,
        };
        assert!(
            is_review_only(&split, &across),
            "two DIFFERENT paragraphs deciding in opposite directions in one operation must be \
             admitted — accept-all over a document holding both kinds is exactly this shape"
        );
    }

    /// `resolved_any`, which is not decoration: a comment-marker-only change satisfies **every
    /// other clause** in `is_review_only`.
    ///
    /// No revision id appears or disappears, and *both* projections are unchanged, because both
    /// drop comment markers. So without the final `resolved_any` a reviewer would be able to
    /// move comment markers through a class that is supposed to be about resolving suggestions —
    /// `comment`'s business reachable through `review`, and a predicate reporting a decision
    /// where none was made.
    #[test]
    fn a_comment_marker_change_is_comment_s_business_and_is_unreachable_through_review() {
        let held = vec![
            run(11, "abc"),
            revision(30, RevisionKind::Insertion, vec![run(31, "xyz")]),
        ];
        let document = holding(held.clone());
        let anchored = vec![
            comment_range(20, 21),
            run(11, "abc"),
            revision(30, RevisionKind::Insertion, vec![run(31, "xyz")]),
            InlineNode::CommentRangeEnd(CommentRangeEnd {
                id: n(22),
                comment: CommentId::new(n(21)),
            }),
        ];

        // The precondition, explicit: every other clause really is satisfied, so this guard is
        // charged to `resolved_any` and not to a projection that happened to differ.
        assert_eq!(
            before_projection(&held),
            before_projection(&anchored),
            "the before-projection must be unchanged, or this guard is not about `resolved_any`"
        );
        assert_eq!(
            after_projection(&held),
            after_projection(&anchored),
            "the after-projection must be unchanged, or this guard is not about `resolved_any`"
        );
        assert_eq!(
            revision_ids(&held),
            revision_ids(&anchored),
            "no revision may appear or disappear, or this guard is not about `resolved_any`"
        );

        assert!(
            !is_review_only(&document, &review(anchored.clone())),
            "moving a comment marker was admitted as a review decision"
        );
        assert!(
            is_comment_only(&document, &review(anchored)),
            "and it must still be a comment, or the gesture is refused to everybody"
        );

        // The same hole with nothing in it at all: an operation that changes nothing.
        let unchanged = review(held);
        assert!(
            !is_review_only(&document, &unchanged),
            "an operation that resolves nothing is not a review decision"
        );
        assert!(is_comment_only(&document, &unchanged));
    }

    /// One **unprotected** paragraph, numbered 10, holding `inlines`.
    ///
    /// Unprotected deliberately: `is_review_only` reads the named paragraph's inlines and
    /// nothing else — no level, no enforcement flag, no settings — so a protected fixture would
    /// add a variable the predicate does not consult and invite a reader to think it does. The
    /// protection-level guards above use [`protected`] for the same reason in reverse.
    fn holding(inlines: Vec<InlineNode>) -> Document {
        document_of(vec![block(10, inlines)])
    }

    /// Two unprotected paragraphs, numbered 10 and 20.
    fn holding_two(first: Vec<InlineNode>, second: Vec<InlineNode>) -> Document {
        document_of(vec![block(10, first), block(20, second)])
    }

    fn block(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: n(id),
            properties: ParagraphProperties::default().into(),
            inlines,
        })
    }

    fn document_of(body: Vec<BlockNode>) -> Document {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("an id");
        Document::new(document_id, body, Definitions::default()).expect("a valid document")
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
                password: None,
            }),
        }
    }

    // ---- the `w:formatting` axis -------------------------------------------------------
    //
    // The second of Word's two restrictions, and the one that was imported, exported, carried
    // through the dialog and enforced by nothing (`165` §7.6). Every guard below was driven
    // RED by deleting the enforcement rather than by hiding a control; the mutations and their
    // output are in the commit message.

    /// The id of the one style these fixtures define.
    fn styled() -> StyleId {
        StyleId::new(n(40))
    }

    /// A document with ONE paragraph, a style table holding [`styled`], and
    /// `w:formatting="1"` — at `edit`, enforced unless stated.
    ///
    /// `locked` is the style's `w:locked`, and `defaults` the
    /// `w:latentStyles/@w:defLockedState` a style the table does NOT define inherits.
    fn formatting_restricted(
        edit: DocumentProtectionEdit,
        enforcement: bool,
        locked: bool,
        default_locked_state: Option<bool>,
    ) -> Document {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("an id");
        let mut definitions = Definitions::default();
        definitions.settings.document_protection = Some(DocumentProtection {
            edit,
            enforcement,
            formatting: true,
            password: None,
        });
        // Built from the crate's own helper rather than a twenty-field literal: a field added
        // to `Style` on a sibling branch is a breaking change to every literal with nothing
        // for a merge to conflict on (`SKILL` §5a shape 1).
        let mut style = crate::references::caption_style(None);
        style.locked = locked;
        definitions.styles.insert(styled(), style);
        if default_locked_state.is_some() {
            definitions.latent_styles = Some(LatentStyles {
                default_locked_state,
                ..LatentStyles::default()
            });
        }
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

    /// Direct character formatting over paragraph 10 — bold, the gesture a formatting
    /// restriction exists to refuse.
    fn embolden() -> Operation {
        Operation::FormatText {
            range: Range {
                start: Pos::new(n(10), 0),
                end: Pos::new(n(10), 4),
            },
            delta: crate::FormatDelta {
                bold: Some(true),
                ..crate::FormatDelta::default()
            },
        }
    }

    /// `SetParagraphProperties` on paragraph 10 installing `properties`.
    fn set_properties(properties: ParagraphProperties) -> Operation {
        Operation::SetParagraphProperties {
            node: n(10),
            properties: Box::new(properties),
        }
    }

    #[test]
    fn a_formatting_restriction_refuses_direct_formatting_and_takes_an_unlocked_style() {
        // The rule, both directions, on a document whose `w:edit` restricts NOTHING — which
        // is the state Word writes when an author ticks the formatting box only, and the
        // state in which this axis was previously a no-op. If the two assertions were not
        // both here, a rule that refused everything would look identical to a correct one.
        let document =
            formatting_restricted(DocumentProtectionEdit::None, true, false, Some(false));

        assert_eq!(
            refuse_if_protected(&document, &[embolden()], Capabilities::local()),
            Err(ProtectionRefusal::FormattingRestricted),
            "direct formatting is the one gesture `w:formatting` names"
        );

        let apply_style = set_properties(ParagraphProperties {
            style_ref: Some(styled()),
            ..ParagraphProperties::default()
        });
        assert_eq!(
            refuse_if_protected(&document, &[apply_style], Capabilities::local()),
            Ok(()),
            "applying an UNLOCKED style is what \"only allow formatting with unlocked \
             styles\" permits; refusing it would make the restriction a read-only one"
        );

        // The projection, which is why this cannot be a variant-level rule: the SAME
        // operation carrying the same style plus one direct property is refused.
        let style_and_more = set_properties(ParagraphProperties {
            style_ref: Some(styled()),
            keep_lines: Some(true),
            ..ParagraphProperties::default()
        });
        assert_eq!(
            refuse_if_protected(&document, &[style_and_more], Capabilities::local()),
            Err(ProtectionRefusal::FormattingRestricted),
            "a style change smuggling direct paragraph formatting beside it is still direct \
             formatting"
        );
    }

    #[test]
    fn a_locked_style_cannot_be_applied_and_the_refusal_says_which_problem_it_is() {
        // `w:locked`'s first consumer in this tree. The assertion that matters is the
        // DISTINCT refusal: a reader told "formatting can only be changed by applying one of
        // its styles" after applying one of its styles has been sent in a circle.
        let document = formatting_restricted(DocumentProtectionEdit::None, true, true, None);
        let apply_style = set_properties(ParagraphProperties {
            style_ref: Some(styled()),
            ..ParagraphProperties::default()
        });
        assert_eq!(
            refuse_if_protected(&document, &[apply_style], Capabilities::local()),
            Err(ProtectionRefusal::StyleLocked)
        );
        assert_ne!(
            ProtectionRefusal::StyleLocked.reason(),
            ProtectionRefusal::FormattingRestricted.reason(),
            "two problems with two remedies may not share one sentence"
        );

        // Removing a style is not applying one, and the document defaults are not the
        // author's locked formatting, so it is allowed.
        assert_eq!(
            refuse_if_protected(
                &document,
                &[set_properties(ParagraphProperties::default())],
                Capabilities::local()
            ),
            Ok(())
        );
    }

    #[test]
    fn a_style_the_table_does_not_define_inherits_the_declared_latent_default() {
        // The branch no `Style` field can answer, and `w:defLockedState`'s first consumer.
        // A style id that resolves to nothing is graded by the latent-styles block, so a
        // document that declares its latent built-ins locked has them locked.
        let absent = StyleId::new(n(41));
        let apply = || {
            set_properties(ParagraphProperties {
                style_ref: Some(absent),
                ..ParagraphProperties::default()
            })
        };

        let locked_by_default =
            formatting_restricted(DocumentProtectionEdit::None, true, false, Some(true));
        assert_eq!(
            refuse_if_protected(&locked_by_default, &[apply()], Capabilities::local()),
            Err(ProtectionRefusal::StyleLocked)
        );

        // And the other direction, so the guard cannot pass by refusing every absent style.
        let unlocked_by_default =
            formatting_restricted(DocumentProtectionEdit::None, true, false, Some(false));
        assert_eq!(
            refuse_if_protected(&unlocked_by_default, &[apply()], Capabilities::local()),
            Ok(())
        );

        // No block at all: `false`, the honest absence, rather than a fail-closed guess.
        let silent = formatting_restricted(DocumentProtectionEdit::None, true, false, None);
        assert_eq!(
            refuse_if_protected(&silent, &[apply()], Capabilities::local()),
            Ok(())
        );
    }

    #[test]
    fn the_two_axes_are_independent_so_a_content_edit_survives_a_formatting_restriction() {
        // THE point of the axis being separate, and the assertion a rule bolted onto the
        // editing level could not satisfy: `w:formatting="1"` with no editing restriction
        // means "write what you like, do not reformat it". Typing and deleting must land.
        let document = formatting_restricted(DocumentProtectionEdit::None, true, true, Some(true));
        for op in [typing(), deleting()] {
            assert_eq!(
                refuse_if_protected(&document, std::slice::from_ref(&op), Capabilities::local()),
                Ok(()),
                "a formatting restriction refused the content edit {op:?}, which is the \
                 other axis"
            );
        }
    }

    #[test]
    fn a_formatting_restriction_that_is_not_enforced_does_not_apply() {
        // The same rule the editing level follows, and the one `w:enforcement` exists for: a
        // restriction an author set up and switched off is retained and not applied.
        let document = formatting_restricted(DocumentProtectionEdit::None, false, true, Some(true));
        assert_eq!(
            refuse_if_protected(&document, &[embolden()], Capabilities::local()),
            Ok(())
        );
    }

    #[test]
    fn redefining_a_style_is_refused_even_when_the_style_is_not_locked() {
        // The bypass this axis would otherwise have, and the reason the sourced-narrow
        // reading was not taken: rewriting `Caption`'s `w:rPr` reformats every paragraph
        // using it, which is strictly more than the one run of bold that IS refused. A rule a
        // caller can walk around is a suggestion.
        let document =
            formatting_restricted(DocumentProtectionEdit::None, true, false, Some(false));
        let mut redefined = crate::references::caption_style(None);
        redefined.run = Some(RunProperties {
            bold: Some(false),
            ..RunProperties::default()
        });
        let op = Operation::SetStyleDefinition {
            id: styled(),
            style: Some(Box::new(redefined)),
        };
        assert_eq!(
            refuse_if_protected(&document, &[op], Capabilities::local()),
            Err(ProtectionRefusal::FormattingRestricted)
        );

        // A LOCKED style's definition is normatively immutable (ECMA-376 §17.7.4.6), and the
        // more specific sentence is the one the reader gets.
        let locked = formatting_restricted(DocumentProtectionEdit::None, true, true, None);
        assert_eq!(
            refuse_if_protected(
                &locked,
                &[Operation::SetStyleDefinition {
                    id: styled(),
                    style: None,
                }],
                Capabilities::local()
            ),
            Err(ProtectionRefusal::StyleLocked)
        );
    }

    #[test]
    fn the_editing_level_outranks_the_formatting_axis_on_a_document_carrying_both() {
        // Both axes refuse this operation and only one sentence is shown. "This document is
        // protected against changes" is the reader's whole situation; "formatting can only be
        // changed by applying one of its styles" invites them to try a style that is also
        // refused. Narrowest authority first, the rule the chrome's badge follows too.
        let document =
            formatting_restricted(DocumentProtectionEdit::ReadOnly, true, false, Some(false));
        assert_eq!(
            refuse_if_protected(&document, &[embolden()], Capabilities::local()),
            Err(ProtectionRefusal::ReadOnly)
        );
    }

    #[test]
    fn lifting_a_formatting_restriction_is_not_refused_by_the_restriction_it_lifts() {
        // The one-way door, on the new axis: a formatting-restricted document must not be
        // permanently unreformattable, which is strictly worse than not enforcing the
        // restriction at all.
        //
        // **What this guard does and does not catch, measured rather than asserted.** Moving
        // the formatting check ABOVE `exempt_from_protection` was tried and left it GREEN,
        // because the axis's `SetDocumentProtection` arm answers `Ok` on its own — the door
        // is held open by that arm's value, not by the ordering, and the ordering is already
        // guarded by `only_the_protection_operation_itself_is_exempt`. What reddens this is
        // moving that arm into the refused set, on the `viewer()` line: the first two
        // assertions are answered by the exemption before the axis is reached, so the
        // UNEXEMPTED caller is the one that can see the arm at all. Recorded because a guard
        // believed to cover an ordering it cannot see is how a rule gets deleted quietly.
        let document = formatting_restricted(DocumentProtectionEdit::None, true, true, Some(true));
        assert_eq!(
            refuse_if_protected(&document, &[lift()], Capabilities::local()),
            Ok(())
        );
        assert_eq!(
            refuse_if_protected(&document, &[lift()], Capabilities::owner()),
            Ok(())
        );
        // A viewer is NOT exempt, so the operation falls through to the axis and the axis
        // answers `Ok` — because a formatting restriction has no standing to stop itself
        // being lifted, which is what this function's `SetDocumentProtection` arm says. The
        // viewer's refusal is the GRANT's, one layer out, and asserting it here would be
        // asserting the wrong authority: this guard first claimed `FormattingRestricted` and
        // went red, which is the difference between the two layers made visible.
        assert_eq!(
            refuse_if_protected(&document, &[lift()], Capabilities::viewer()),
            Ok(()),
            "the policy axis must not be the thing that refuses a viewer"
        );
        assert!(
            crate::access::refuse_if_not_permitted(
                Some(&document),
                &[lift()],
                Capabilities::viewer()
            )
            .is_err(),
            "a viewer must still be refused — by the grant, which is the outer gate"
        );
    }

    #[test]
    fn every_protection_refusal_carries_a_distinct_routable_code_and_sentence() {
        // The class guard rather than five row guards: a new level or axis that reuses
        // another's code reaches the chrome's `PROTECTION_REFUSAL_KEYS` table as a collision
        // and the reader is told the wrong thing. `#[non_exhaustive]` means this list is
        // maintained by hand, so the count is asserted beside it.
        let all = [
            ProtectionRefusal::ReadOnly,
            ProtectionRefusal::CommentsOnly,
            ProtectionRefusal::TrackedChangesOnly,
            ProtectionRefusal::FormattingRestricted,
            ProtectionRefusal::StyleLocked,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for refusal in all {
            let reason = refusal.reason();
            assert!(
                seen.insert(reason),
                "{refusal:?} reuses another refusal's sentence: {reason}"
            );
            assert!(
                reason.contains("protected"),
                "{refusal:?} does not name the document's protection: {reason}"
            );
            // The one claim this family may never make. `w:documentProtection` is plain-text
            // XML and ISO/IEC 29500-1 §17.15.1.29's own note says it "is not intended as a
            // security feature"; a sentence that reads as one would be a false claim in the
            // place a reader is most likely to believe it.
            for forbidden in ["secure", "encrypt", "password", "safe"] {
                assert!(
                    !reason.to_ascii_lowercase().contains(forbidden),
                    "{refusal:?} claims {forbidden:?}, which this protection is not: {reason}"
                );
            }
        }
    }
}
