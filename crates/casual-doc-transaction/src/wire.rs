// SPDX-License-Identifier: Apache-2.0

//! An operation on the wire, and the identity discipline that keeps two replicas from
//! minting the same id for two different nodes.
//!
//! # The hazard, measured rather than assumed
//!
//! The sibling engine records this as its ADR-025: *an interned id is replica-local; the
//! value crosses the wire, the id never does.* A `StringId(3)` means **the sender's** third
//! string, so the receiver must be handed the value and re-intern it locally. They found it
//! as a production defect three separate times.
//!
//! Our shape is different, and the difference was verified before this module was written:
//!
//! 1. **Our operations already carry their values.** `SetStyleDefinition` carries the whole
//!    `Style` beside the `StyleId`, `CreateBookmark` carries the name, `InsertNote` carries
//!    the note's blocks, `InsertFieldRange` carries the definition. There is no operation
//!    in the set of 55 that names a definition it does not either carry or that the total
//!    order guarantees already exists. So there is **no side table to add to the wire op**,
//!    and adding one would be redundant bytes.
//! 2. **`v1::intern` is not an id table.** It is `Shared<T>` — an `Arc` flyweight that
//!    "serializes exactly as `T` does". It puts no id on the wire at all, so it carries
//!    none of this hazard.
//! 3. **The hazard we do have is worse, and it is at the mint.** Every one of `StyleId`,
//!    `BookmarkId`, `FieldRangeId`, `NoteId`, `SectionId`, `HeaderFooterId`, `CommentId`,
//!    `MediaId` and the numbering ids is a newtype over [`NodeId`], which is
//!    `(namespace: u64, counter: u64)`. The live editor derives that namespace **from the
//!    document** — `(document.id() >> 64) ^ 0xED17_ED17_ED17_ED17` — and starts its counter
//!    at one. Two replicas of one document therefore mint **identical ids for different
//!    nodes**, from the first edit. That is not an id that means something else on the
//!    receiver; it is two nodes with one name.
//!
//! # The rule, and why it needs no wire field
//!
//! **An introduction needs a private space; a reference needs the order.**
//!
//! - *Introductions* — the ids an operation mints — must come from a space nobody else
//!   mints in. [`space_of`] derives one per participant from the document's own space and
//!   the server-assigned [`ClientId`], and it is **injective in the client id**, so two
//!   participants can never collide. It is derived rather than carried, so a receiver
//!   computes the sender's space from the [`Arrival`](crate::protocol::Arrival)'s `client`
//!   field and nothing has to be trusted.
//! - *References* — the ids an operation names but did not mint — are safe because the
//!   session is **totally ordered** and everybody starts from one snapshot: the operation
//!   that created a definition is ordered before any operation that names it.
//!
//! [`WireOperation::localise`] enforces the first half and is the choke point for it.
//!
//! # The standing rule for the next interned table
//!
//! Any new definition table added to `v1::Definitions` must, in the same change:
//!
//! 1. have the operation that creates an entry **carry the value**, not just the key;
//! 2. mint its key through the session's [`IdSpace`], never through a document-derived one;
//! 3. add its variant to `WireOperation::introduces` — which is an exhaustive match, so
//!    this one is a compile error rather than a review comment;
//! 4. add its table to [`Table`] and to `localise`'s collision check;
//! 5. arrive with **a test in which the receiver already holds a different entry at that
//!    id**. An id that lines up by accident proves nothing, and a test that merely round
//!    trips a value proves less.

use casual_doc_edit::{Mint, Operation};
use casual_doc_model::NodeId;
use casual_doc_model::v1::Document;
use serde::{Deserialize, Serialize};

use crate::protocol::ClientId;

/// The namespace partition that keeps two replicas from minting the same id.
///
/// **It lives in `casual-doc-model`, not here.** Identity is a property of the model, and
/// the live editor has to mint in a partitioned space whether or not it is in a session —
/// so putting the partition in this crate would have made single-user editing depend on the
/// collaboration modules, which `the_live_editor_has_no_collaboration_dependency` forbids
/// for good reason. Re-exported so `wire::IdSpace` keeps resolving.
///
/// In a session the participant's space is [`IdSpace::participant`] applied to the
/// relay-assigned [`ClientId`]; with no session it is [`IdSpace::local`], a space
/// `IdSpace::participant` never returns.
///
/// **Why not re-map on receipt**, which is what the sibling does for an interned value: the
/// id *is* the identity here, so re-mapping would leave the two replicas disagreeing about
/// the name of the same logical node, and a snapshot could never be compared byte for byte
/// across replicas. Doc 150 §9.3 named that as the blocker for persisted collaboration;
/// re-mapping would make it permanent.
pub use casual_doc_model::IdSpace;

/// The space `client` mints in, given the document's own space.
///
/// A one-line adapter from this crate's participant number to the model's partition, so
/// there is one derivation and not two. `None` for the two participant numbers that would
/// alias a reserved space (see [`IdSpace::participant`]).
#[must_use]
pub fn space_of(base: IdSpace, client: ClientId) -> Option<IdSpace> {
    IdSpace::participant(base, client.get())
}

/// The space a document's own nodes live in.
#[must_use]
pub fn document_space(document: &Document) -> IdSpace {
    IdSpace::of_document(document.id())
}

/// One operation as it travels, together with the identities it introduces.
///
/// The identities are computed once by the sender and carried, rather than recomputed by
/// the receiver, for one reason: the receiver must be able to check what the sender
/// *claimed* to introduce against what its operation actually names. A sender that
/// under-declares is caught by [`WireOperation::localise`] recomputing and comparing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireOperation {
    operation: Operation,
    mint: Mint,
    intent: crate::Intent,
    introduces: Vec<NodeId>,
}

impl WireOperation {
    /// An operation prepared for the wire, with the space it mints in.
    ///
    /// `mint` is not decoration: applying an operation can create nodes (a `FormatText`
    /// splits a run and the tail is a new one), and a receiver that minted those from its
    /// own generator would end up with the same document under different node names. The
    /// space travels so the names do not diverge — doc 150 §9.3.
    #[must_use]
    pub fn of(operation: Operation, mint: Mint) -> Self {
        let introduces = Self::introduces(&operation);
        Self {
            operation,
            mint,
            intent: crate::Intent::NONE,
            introduces,
        }
    }

    /// The same wire operation, carrying what its author declared about it.
    ///
    /// An [`Intent`](crate::Intent) is an **authoring** fact — the block an index was counted
    /// to, the side of a boundary content belongs to — and a receiver cannot reconstruct one,
    /// which is why it travels (doc 150 §9.1/§9.2, ADR-056). It is deliberately not part of
    /// [`WireOperation::of`]: declaring nothing is valid and is what every existing sender
    /// does, so this is additive in the same way the type itself is.
    ///
    /// Unlike `introduces`, an intent is **not** recomputed and compared on receipt. There is
    /// nothing to compare it against: only the sender knows it. What stops it being abused is
    /// that it can only ever make a transform *answer a pair it would otherwise refuse* — it
    /// cannot redirect an operation, because the index it resolves to is read from the
    /// receiver's own document.
    #[must_use]
    pub fn declaring(mut self, intent: crate::Intent) -> Self {
        self.intent = intent;
        self
    }

    /// What the sender declared about this operation, or [`Intent::NONE`](crate::Intent::NONE).
    #[must_use]
    pub const fn intent(&self) -> crate::Intent {
        self.intent
    }

    /// The operation, unchanged.
    #[must_use]
    pub const fn operation(&self) -> &Operation {
        &self.operation
    }

    /// A wire operation whose declaration is whatever the caller says it is.
    ///
    /// Test-only, and it exists for one test: a sender that under-declares what it
    /// introduces. That sender cannot be written with [`WireOperation::of`] — which is the
    /// point — so without this the recomputation in [`WireOperation::localise`] would be
    /// unreachable code that nothing could prove necessary.
    #[cfg(test)]
    pub(crate) fn forged(operation: Operation, mint: Mint, introduces: Vec<NodeId>) -> Self {
        Self {
            operation,
            mint,
            intent: crate::Intent::NONE,
            introduces,
        }
    }

    /// The space this operation mints in, as its sender declared it.
    #[must_use]
    pub const fn mint(&self) -> Mint {
        self.mint
    }

    /// The identities this operation brings into existence, as its sender declared them.
    #[must_use]
    pub fn declared(&self) -> &[NodeId] {
        &self.introduces
    }

    /// The operation as its sender wrote it, **unchecked**.
    ///
    /// # Why this exists, and why it is not [`WireOperation::localise`]
    ///
    /// `localise` is the apply path: it verifies the sender's declarations against a document
    /// and is the only way to get an operation that is safe to *apply*. This is for a caller
    /// that must decide something about the operation and **holds no document** — which is
    /// exactly one caller, the relay, and exactly one question, what
    /// [`access::admitted_by`](casual_doc_edit::access::admitted_by) answers: which capability
    /// class could legitimately have sent this (ADR-060).
    ///
    /// # Does this break ADR-047?
    ///
    /// No, and the line is worth stating because it is a thin one. ADR-047 says the relay holds
    /// no document and runs no transform. Reading which *variant* an operation is costs no
    /// document, no state and no interpretation of content; the exhaustive judgement stays in
    /// `casual-doc-edit`, where `Operation` lives, so a 59th operation is a compile error there
    /// rather than a permission hole here. What the relay must not do is decide anything that
    /// needs the document — and `access` makes that structural by taking the document as an
    /// `Option` and giving the `None` caller a provably weaker answer.
    ///
    /// **Unchecked** is the word that matters: nothing here has verified that the operation can
    /// be applied, that its ids are the sender's to mint, or that it means what it says. Only
    /// [`WireOperation::localise`] does that, and a caller that applies this value instead has
    /// skipped the id-space rule.
    #[must_use]
    pub const fn as_offered(&self) -> &Operation {
        &self.operation
    }

    /// Accepts this operation from `sender`, or says why it cannot be applied.
    ///
    /// Two checks, and both are about identity rather than about content:
    ///
    /// 1. every id the operation introduces was minted in `sender`'s own space, so it
    ///    cannot be an id this replica minted or one a third participant will;
    /// 2. no definition table on `document` already holds an entry at one of those ids.
    ///
    /// The second is not redundant with the first. It is the check that catches a session
    /// whose id-space discipline was never established — which is today's live editor,
    /// whose minting namespace is derived from the document and is therefore *the same on
    /// every replica*. Without it, an arriving `SetStyleDefinition` **silently replaces**
    /// the receiver's own style of the same id, because `apply` treats `Some(style)` as
    /// "insert or replace" by design.
    ///
    /// # Errors
    ///
    /// [`Collision`] naming the id and what it clashed with. The session turns that into
    /// [`Refusal::IdCollision`](crate::protocol::Refusal::IdCollision) / `ODC-7008`.
    ///
    /// # Complexity
    ///
    /// O(ids introduced × log definitions). No document walk: the definition tables are
    /// keyed maps, and the id-space test is arithmetic.
    ///
    /// # What this does not see
    ///
    /// Stated plainly, because a guard's blind spots are part of its contract:
    ///
    /// - **Ids inside a carried subtree.** `InsertBlocks`, `InsertTable`, `SetInlines`,
    ///   `ReplaceTable`, `InsertRow`, `InsertColumn` and the note/field/header payloads
    ///   carry whole `BlockNode`/`InlineNode` trees whose nodes have ids of their own.
    ///   Enumerating them means a recursive walk of 4 block and 28 inline variants, which
    ///   would duplicate the model's own structure in this crate. They are covered by the
    ///   id-space rule at the *mint* and, if that discipline is broken, by
    ///   `Document::validate`'s duplicate-node-id rule — which is O(document) and therefore
    ///   deliberately off the apply path (doc 147 §3.2), so such a collision lands first and
    ///   is only caught at the next validation point. Recorded as doc 152 §10 Q2.
    /// - **References.** An operation naming a definition it did not mint is accepted here;
    ///   the total order is what makes that safe, and this module cannot check an order.
    pub fn localise(&self, document: &Document, sender: IdSpace) -> Result<&Operation, Collision> {
        // Recomputed rather than trusted: a sender that under-declares would otherwise skip
        // both checks below for the id it left out, which is the one that matters.
        let actual = Self::introduces(&self.operation);
        if let Some(&id) = actual.iter().find(|id| !self.introduces.contains(id)) {
            return Err(Collision {
                id,
                clash: Clash::Undeclared,
            });
        }
        // The mint is checked exactly as a declared id is, and for the same reason: every
        // identity `apply` creates comes out of this space, so a sender that mints outside
        // its own namespace would name nodes a third participant is entitled to name.
        if !sender.holds(self.mint.base()) {
            return Err(Collision {
                id: self.mint.base(),
                clash: Clash::ForeignSpace { sender },
            });
        }
        for id in actual {
            if !sender.holds(id) {
                return Err(Collision {
                    id,
                    clash: Clash::ForeignSpace { sender },
                });
            }
            if let Some(table) = held_by(document, id) {
                return Err(Collision {
                    id,
                    clash: Clash::AlreadyHeld { table },
                });
            }
        }
        Ok(&self.operation)
    }

    /// A lower bound on the bytes this operation's payload occupies in any codec.
    ///
    /// # What it is for, and why a shallow measure is enough
    ///
    /// It exists to pack a **backlog** into chunks (`protocol::CHUNK_BUDGET_BYTES`), not to
    /// enforce a frame cap. A single commit larger than the budget is admitted deliberately
    /// — otherwise `flush` would spin on a submission it can never make — so the measure's
    /// precision changes how well several commits are packed and never whether an over-cap
    /// commit is sent. Enforcing a transport's frame cap belongs to the codec lane, with the
    /// cap stated once so both ends read the same number.
    ///
    /// So this counts the text an operation names **directly** and charges a constant per
    /// identity and per top-level payload item. It does not walk a carried subtree, for the
    /// reason `localise` gives.
    #[must_use]
    pub fn carried_bytes(&self) -> usize {
        /// A fixed charge per identity and per carried item, so the measure grows with a
        /// payload that carries no text.
        const PER_ITEM: usize = 32;

        // The mint is one identity's worth of bytes, and it is on every operation.
        let ids = (self.introduces.len() + 1) * PER_ITEM;
        let payload = match &self.operation {
            Operation::InsertText { text, .. } => text.len(),
            Operation::RenameBookmark { name, .. } => name.len(),
            Operation::CreateBookmark { name, .. } => name.len(),
            Operation::SetHyperlink {
                target, tooltip, ..
            } => tooltip.as_ref().map_or(0, String::len) + target.as_ref().map_or(0, |_| PER_ITEM),
            Operation::SetObjectDescr { descr, .. } => descr.as_ref().map_or(0, String::len),
            Operation::SetInlines { inlines, .. } => inlines.len() * PER_ITEM,
            Operation::InsertBlocks { blocks, .. }
            | Operation::InsertFieldRange { blocks, .. }
            | Operation::InsertNote { blocks, .. }
            | Operation::CreateHeaderFooterBody { blocks, .. } => blocks.len() * PER_ITEM,
            Operation::InsertColumn { cells, .. } => cells.len() * PER_ITEM,
            Operation::InsertRow { row, .. } => row.cells.len() * PER_ITEM,
            Operation::InsertTable { table, .. } => table.rows.len() * PER_ITEM,
            Operation::ReplaceTable { replacement, .. } => replacement.rows.len() * PER_ITEM,
            Operation::InsertField { field, .. } => field.instruction.len(),
            Operation::UpdateReviewState {
                paragraphs,
                comments,
            } => {
                paragraphs.len() * PER_ITEM
                    + comments.as_ref().map_or(0, |map| map.len() * PER_ITEM)
            }
            Operation::SetAbstractNumbering { definition, .. } => definition
                .as_ref()
                .map_or(0, |definition| definition.levels.len() * PER_ITEM),
            Operation::SetNumberingInstance { instance, .. } => instance
                .as_ref()
                .map_or(0, |instance| instance.overrides.len() * PER_ITEM),
            Operation::SetMediaReference { reference, .. } => {
                reference.as_ref().map_or(0, |reference| {
                    reference.part_name.len()
                        + reference.relationship_id.len()
                        + reference.media_type.len()
                })
            }
            Operation::SetStyleDefinition { style, .. } => style.as_ref().map_or(0, |style| {
                style.name.as_ref().map_or(0, String::len) + PER_ITEM
            }),
            // The widest payload in the op set, and the one it would be most wrong to
            // charge as fixed-size: a projection carries every cached data point, so a
            // 5,000-point chart is thousands of items on the wire however few fields the
            // struct has. Bounded by the chart model's own limits; O(series + points).
            Operation::SetChartDefinition { chart, .. } => chart.as_ref().map_or(0, |chart| {
                let groups = &chart.plot_area.groups;
                let series: usize = groups.iter().map(|group| group.series.len()).sum();
                let points: usize = chart.data_ranges().map(|range| range.points.len()).sum();
                (groups.len() + chart.plot_area.axes.len() + series + points) * PER_ITEM
            }),
            // Every remaining operation carries a bounded, fixed-size payload: offsets, a
            // property bundle, a geometry, a flag, or nothing but the ids already charged
            // above. Grouped rather than wildcarded so a 56th variant is a compile error
            // here too, and whoever adds it has to decide whether it carries bytes.
            // `SetDocumentProtection` belongs here and not above: two bools and a
            // five-valued enum, `Copy`, with no owned payload at all (ADR-059).
            Operation::SetDocumentProtection { .. }
            | Operation::DeleteText { .. }
            | Operation::SplitParagraph { .. }
            | Operation::JoinParagraphs { .. }
            | Operation::FormatText { .. }
            | Operation::ClearFormatting { .. }
            | Operation::SetParagraphProperties { .. }
            | Operation::DeleteRow { .. }
            | Operation::DeleteColumn { .. }
            | Operation::DeleteTable { .. }
            | Operation::DeleteBlocks { .. }
            | Operation::SetExtent { .. }
            | Operation::SetGroupGeometry { .. }
            | Operation::SetAnchor { .. }
            | Operation::SetImageCrop { .. }
            | Operation::DeleteObject { .. }
            | Operation::InsertObjectNode { .. }
            | Operation::InsertInlineObject { .. }
            | Operation::RemoveInlineObject { .. }
            | Operation::SetTableCellProperties { .. }
            | Operation::SetTableProperties { .. }
            | Operation::SetCoreProperties { .. }
            | Operation::SetSectionGeometry { .. }
            | Operation::SpliceSectionBoundary { .. }
            | Operation::DeleteBookmark { .. }
            | Operation::RemoveField { .. }
            | Operation::RemoveFieldRange { .. }
            | Operation::RemoveNote { .. }
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
            | Operation::SetObjectLocks { .. } => 0,
        };
        PER_ITEM + ids + payload
    }

    /// The identities an operation brings into existence.
    ///
    /// **Exhaustive with no wildcard arm**, so a 56th operation is a compile error here and
    /// whoever adds it has to say whether it mints anything. That is the same enforcement
    /// `transform`'s seven matches use, for the same reason: a rule kept in a doc comment in
    /// another crate is not checked by anything.
    ///
    /// Ids inside a carried subtree are not enumerated; `localise` states why and doc 152
    /// §10 Q2 records it.
    fn introduces(operation: &Operation) -> Vec<NodeId> {
        match operation {
            Operation::SplitParagraph { new_id, .. } => vec![*new_id],
            Operation::SetHyperlink { id, .. } => vec![*id],
            Operation::CreateBookmark {
                bookmark,
                start_id,
                end_id,
                ..
            } => vec![bookmark.node_id(), *start_id, *end_id],
            Operation::InsertField { field, .. } => vec![field.id],
            Operation::InsertFieldRange { field, .. } => vec![field.node_id()],
            Operation::InsertNote {
                note, reference_id, ..
            } => vec![note.node_id(), *reference_id],
            Operation::CreateHeaderFooterBody { id, .. } => vec![id.node_id()],
            // `Some(style)` inserts OR replaces, so the id is an introduction only when the
            // receiver does not already hold it — which is exactly what `localise` decides.
            // Declaring it here is what makes the silent replace reachable by a check.
            Operation::SetStyleDefinition { id, style } => {
                style.as_ref().map_or_else(Vec::new, |_| vec![id.node_id()])
            }
            // The same rule as the style table above, for the same reason: `Some(_)` inserts
            // OR replaces, so declaring the id is what makes a silent replace reachable by
            // `localise`'s already-held check.
            Operation::SetAbstractNumbering { id, definition } => definition
                .as_ref()
                .map_or_else(Vec::new, |_| vec![id.node_id()]),
            Operation::SetNumberingInstance { id, instance } => instance
                .as_ref()
                .map_or_else(Vec::new, |_| vec![id.node_id()]),
            Operation::SetMediaReference { id, reference } => reference
                .as_ref()
                .map_or_else(Vec::new, |_| vec![id.node_id()]),
            // Same rule again: a `ChartId` IS a `NodeId` the editor mints, so `Some(_)`
            // declares it and `localise` decides whether the receiver already holds it.
            Operation::SetChartDefinition { id, chart } => chart
                .as_ref()
                .map_or_else(Vec::new, |_| vec![id.node_id()]),
            Operation::SpliceSectionBoundary { boundary, .. } => boundary
                .as_ref()
                .map_or_else(Vec::new, |boundary| vec![boundary.id.node_id()]),
            Operation::UpdateReviewState { comments, .. } => {
                comments.as_ref().map_or_else(Vec::new, |map| {
                    map.iter().map(|(id, _)| id.node_id()).collect()
                })
            }
            // Introduces nothing: it names what already exists, or removes it, or carries a
            // subtree whose ids this match deliberately does not enumerate.
            Operation::InsertText { .. }
            | Operation::DeleteText { .. }
            | Operation::JoinParagraphs { .. }
            | Operation::FormatText { .. }
            | Operation::ClearFormatting { .. }
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
            // It carries a policy value, not an identity: there is no id for a receiver to
            // already hold, so there is no silent replace to make reachable (ADR-059).
            | Operation::SetDocumentProtection { .. }
            | Operation::SetSectionGeometry { .. }
            | Operation::DeleteBookmark { .. }
            | Operation::RenameBookmark { .. }
            | Operation::RemoveField { .. }
            | Operation::RemoveFieldRange { .. }
            | Operation::RemoveNote { .. }
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
            | Operation::SetObjectLocks { .. } => Vec::new(),
        }
    }
}

/// A definition table an arriving identity clashed with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Table {
    /// `Definitions::styles`.
    Styles,
    /// `Definitions::bookmarks`.
    Bookmarks,
    /// `Definitions::field_ranges`.
    FieldRanges,
    /// `Definitions::footnotes` or `Definitions::endnotes`.
    Notes,
    /// `Definitions::headers` or `Definitions::footers`.
    HeadersFooters,
    /// `Definitions::comments`.
    Comments,
    /// `Definitions::sections`.
    Sections,
    /// `Definitions::media`.
    Media,
    /// `Definitions::abstract_numbering` or `Definitions::numbering`.
    Numbering,
}

/// Why an arriving identity cannot be accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Clash {
    /// The id was not minted in the sender's own space, so the discipline that makes ids
    /// unique across replicas was not followed and nothing here can make it true after the
    /// fact.
    ForeignSpace {
        /// The space the sender was supposed to mint in.
        sender: IdSpace,
    },
    /// This replica already holds a definition at that id. Applying the operation would
    /// overwrite somebody's node with somebody else's.
    AlreadyHeld {
        /// Where it is held.
        table: Table,
    },
    /// The operation introduces an id its sender did not declare, so neither check above
    /// was applied to it.
    Undeclared,
}

/// An arriving identity that cannot be accepted, and why.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Collision {
    /// The identity.
    pub id: NodeId,
    /// What it clashed with.
    pub clash: Clash,
}

/// Which definition table, if any, already holds `id`.
///
/// O(log n) per table. The section list is a `Vec` and is scanned, which is O(sections) —
/// bounded by the document's section count, not by its size.
fn held_by(document: &Document, id: NodeId) -> Option<Table> {
    use casual_doc_model::v1::{
        AbstractNumberingId, BookmarkId, CommentId, FieldRangeId, HeaderFooterId, MediaId, NoteId,
        NumberingInstanceId, StyleId,
    };

    let definitions = document.definitions();
    if definitions.styles.contains_key(&StyleId::new(id)) {
        return Some(Table::Styles);
    }
    if definitions.bookmarks.contains_key(&BookmarkId::new(id)) {
        return Some(Table::Bookmarks);
    }
    if definitions
        .field_ranges
        .contains_key(&FieldRangeId::new(id))
    {
        return Some(Table::FieldRanges);
    }
    if definitions.footnotes.contains_key(&NoteId::new(id))
        || definitions.endnotes.contains_key(&NoteId::new(id))
    {
        return Some(Table::Notes);
    }
    if definitions.headers.contains_key(&HeaderFooterId::new(id))
        || definitions.footers.contains_key(&HeaderFooterId::new(id))
    {
        return Some(Table::HeadersFooters);
    }
    if definitions.comments.contains_key(&CommentId::new(id)) {
        return Some(Table::Comments);
    }
    if definitions.media.contains_key(&MediaId::new(id)) {
        return Some(Table::Media);
    }
    if definitions
        .abstract_numbering
        .contains_key(&AbstractNumberingId::new(id))
        || definitions
            .numbering
            .contains_key(&NumberingInstanceId::new(id))
    {
        return Some(Table::Numbering);
    }
    if definitions
        .sections
        .iter()
        .any(|section| section.id.node_id() == id)
    {
        return Some(Table::Sections);
    }
    None
}
