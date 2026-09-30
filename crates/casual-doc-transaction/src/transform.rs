// SPDX-License-Identifier: Apache-2.0

//! Operational transform over the closed op set — the concurrency primitive from
//! ADR-033 and [doc 150](../../../docs/150-OPERATIONAL-TRANSFORM-OVER-THE-CLOSED-OP-SET.md).
//!
//! [`transform`] answers one question: *`subject` was written against a state where
//! `against` had not happened; what is `subject` on a state where it has?* With a server
//! imposing the total order that is the whole of the algorithm — a client rebases its
//! pending operations onto each revision as it arrives, and the server rebases an
//! incoming operation onto everything committed since the revision it was based on.
//!
//! # The property this must satisfy
//!
//! **TP1**, convergence:
//!
//! ```text
//! apply(apply(S, a), transform(b, a, Later)) == apply(apply(S, b), transform(a, b, Earlier))
//! ```
//!
//! Two clients that saw the same state and edited concurrently end up with the same
//! document, whichever order the server picked. It is asserted as a property over
//! generated pairs rather than by example, because transform functions fail on the pair
//! nobody thought of and that is the standard way OT ships broken.
//!
//! **TP2 — the property peer-to-peer OT needs — is deliberately not provided.** A server
//! orders everything, which is what removes the obligation (ADR-033). If peer-to-peer or
//! merge-after-long-divergence is ever required, this design is void and needs a new ADR
//! rather than an extension.
//!
//! # Why a [`Change`] and not an [`Operation`]
//!
//! A spreadsheet delete names its victims by address, so the operation describes itself
//! completely. A document delete names them by **identity**, and the identities are in the
//! *inverse*: `DeleteBlocks { container, index, count }` does not say which nodes died;
//! its inverse `InsertBlocks { .., blocks }` names every one. `JoinParagraphs` does not say
//! where the join boundary fell; its inverse `SplitParagraph { at, .. }` does.
//!
//! Invariant I2 (doc 45) already requires every operation to return its inverse, and
//! [`Commit`](crate::Commit) already retains it. So transform is defined over the pair, and
//! stays a pure function of data the log already holds — no document, no lookup. That is
//! the one structural difference from the sibling spreadsheet engine's transform, which
//! passes side tables in instead.
//!
//! # What this will not answer
//!
//! Four shapes return [`TransformError::Unsupported`] rather than a guess, because an
//! untransformed operation applied to a state it was not written against diverges the
//! replicas *quietly*, and quiet divergence is the one outcome this layer must not produce.
//! The list is doc 150 §6 and is pinned by `the_refusal_surface_is_exactly_these_cases`,
//! which fails when a refusal is added without being written there.
//!
//! - **U1** — a [`Operation::SetHyperlink`] whose range straddles a concurrent paragraph
//!   split or join. Two wrappers are needed and the operation carries one fresh id.
//! - **U2** — a sibling-indexed operation meeting a concurrent split or join, with no
//!   [`BlockPlacement`]. Neither operation says where the other sits among its container's
//!   blocks. Answered by [`transform_placed`].
//! - **U3** — a [`Operation::JoinParagraphs`] whose two paragraphs a concurrent band
//!   separated. No operation joins non-adjacent paragraphs.
//! - **U4** — an inline-index operation meeting a change to the same paragraph's inline
//!   membership expressed as a byte offset. The index the run-splitting produced is not on
//!   either operation.
//!
//! # Cost
//!
//! [`transform`] is **O(1) in document size**. The only unbounded term is the walk over
//! `against.inverse`'s retained payload when the concurrent change removed content, which
//! is O(nodes that change removed) — bounded by what the concurrent operation itself
//! carried, never by the document. Rebasing one arriving operation is therefore
//! O(operations since its base revision), which is `107` B2 with nothing hidden inside.
//!
//! **Single-user pays nothing.** Nothing in the engine calls this; a lone editor allocates
//! no transform state and runs no transform. `107` exit gate 7 and doc 150 §7.

use std::collections::HashMap;

use casual_doc_edit::{FormatDelta, Operation, Pos, Range as EditRange, RunningRegion};
use casual_doc_model::NodeId;
// Separate `use` lines for the three definition tables an editing command may now
// introduce (`147`, ADR-005) — added apart from the shared sorted block so parallel lanes
// do not conflict in it.
use casual_doc_model::v1::AbstractNumberingId;
use casual_doc_model::v1::MediaId;
use casual_doc_model::v1::NumberingInstanceId;
use casual_doc_model::v1::{
    BlockNode, BookmarkId, Document, HeaderFooterKind, InlineNode, SectionId, StyleId,
};

/// Where `subject` sits relative to `against` in the order the server settled on.
///
/// Both replicas run this function and each must reach the same answer about a contested
/// boundary. "Whoever is transforming" cannot decide it — that is the one fact the two
/// sides disagree about. The settled order is the shared fact, so it is what the tie-break
/// reads.
///
/// A client rebasing its own unacknowledged operation onto an arriving server operation
/// uses [`Side::Later`] for its own; rebasing the server's operation onto its pending one
/// uses [`Side::Earlier`] for the server's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    /// `subject` is ordered before `against`, so it holds a contested boundary and yields
    /// a contested write.
    Earlier,
    /// `subject` is ordered after `against`, so it moves off a contested boundary and wins
    /// a contested write.
    Later,
}

impl Side {
    /// The same relationship seen from the other operation.
    #[must_use]
    pub const fn flip(self) -> Self {
        match self {
            Self::Earlier => Self::Later,
            Self::Later => Self::Earlier,
        }
    }
}

/// Every reason [`transform`] can refuse for — doc 150 §6.
///
/// Refusing is a first-class answer: an untransformed operation applied to a state it was
/// not written against diverges the replicas *quietly*, and quiet divergence is the one
/// outcome this layer must not produce. So the surface is enumerated rather than
/// discovered, `the_refusal_surface_is_exactly_these_cases` fails when a refusal reaches
/// the surface without being listed here, and a companion check fails the build if a
/// refusal is written as a bare string literal instead of a constant.
pub const REFUSAL_REASONS: [&str; 14] = [
    MALFORMED_CHANGE,
    HYPERLINK_CANNOT_DIVIDE,
    SPLIT_SEPARATED_THE_JOIN,
    INSERTION_SEPARATED_THE_JOIN,
    NO_PLACEMENT_FOR_SPLIT,
    NO_PLACEMENT_FOR_JOIN,
    NO_PLACEMENT_FOR_ADJACENCY,
    JOIN_MERGED_INTO_A_REMOVAL,
    INLINE_INDEX_ACROSS_TEXT,
    INLINE_INDEX_ACROSS_MEMBERSHIP,
    INSERTION_AT_AN_ABSORBED_PARAGRAPH_START,
    REWRITE_MEETS_A_STRUCTURAL_EDIT,
    REWRITE_MEETS_A_POSITIONAL_EDIT,
    TABLE_AXIS_NEEDS_A_FRESH_CELL,
];

/// The change's inverse does not describe what its operation did, so the change cannot say
/// what it moved or destroyed. `Coalesce::ContinueKeepingFirstInverse` is the shape that
/// produces this (doc 150 §6 U4).
const MALFORMED_CHANGE: &str =
    "the recorded inverse does not describe what the concurrent operation did";
/// U1.
const HYPERLINK_CANNOT_DIVIDE: &str =
    "a hyperlink cannot be divided in two: the operation carries one fresh id";
/// U3, via a split.
const SPLIT_SEPARATED_THE_JOIN: &str = "a concurrent split separated the two paragraphs, and no operation joins non-adjacent paragraphs";
/// U3, via an insertion.
const INSERTION_SEPARATED_THE_JOIN: &str = "a concurrent insertion separated the two paragraphs, and no operation joins non-adjacent paragraphs";
/// U2, for a split.
const NO_PLACEMENT_FOR_SPLIT: &str =
    "no block placement: neither operation says where the split paragraph sits";
/// U2, for a join.
const NO_PLACEMENT_FOR_JOIN: &str =
    "no block placement: neither operation says where the joined paragraph sat";
/// U2, for the adjacency a join requires.
const NO_PLACEMENT_FOR_ADJACENCY: &str =
    "no block placement: a concurrent band may have separated the two paragraphs";
/// U6.
const JOIN_MERGED_INTO_A_REMOVAL: &str = "a concurrent join merged a paragraph this removal spans, and no operation removes the text it merged in";
/// U4, for a text edit.
const INLINE_INDEX_ACROSS_TEXT: &str =
    "an inline index cannot be rebased across a text edit in the same paragraph";
/// U4, for a zero-width inline.
const INLINE_INDEX_ACROSS_MEMBERSHIP: &str =
    "an inline index cannot be rebased across an inline added or removed at a byte offset";
/// U5.
const INSERTION_AT_AN_ABSORBED_PARAGRAPH_START: &str = "text inserted at the start of a paragraph a concurrent join absorbed has no expressible side: `Pos` carries no affinity";
/// U9.
const TABLE_AXIS_NEEDS_A_FRESH_CELL: &str = "a concurrent insertion on the other table axis needs one more cell than the operation carries, and a transform may not mint identities";
/// U8.
const REWRITE_MEETS_A_POSITIONAL_EDIT: &str = "a whole-paragraph rewrite meets a concurrent edit inside that paragraph, and only a rewrite that contained the other edit would converge";
/// U7.
const REWRITE_MEETS_A_STRUCTURAL_EDIT: &str = "a whole-paragraph rewrite meets a concurrent split or join of that paragraph, and no operation removes the half the split carved off";

/// One committed change, as transform must see it: the operation and the inverse
/// `casual_doc_edit::apply` returned for it.
///
/// See the module docs for why the inverse is not optional. A commit whose inverses were
/// dropped ([`Coalesce::ContinueKeepingFirstInverse`](crate::Coalesce)) cannot describe
/// what it destroyed and so cannot serve as `against`; doc 150 §10 Q2.
#[derive(Clone, Copy, Debug)]
pub struct Change<'a> {
    /// The operation that was applied.
    pub operation: &'a Operation,
    /// The inverse `casual_doc_edit::apply` returned for it.
    pub inverse: &'a Operation,
}

impl<'a> Change<'a> {
    /// A change from an operation and the inverse that was recorded for it.
    #[must_use]
    pub const fn new(operation: &'a Operation, inverse: &'a Operation) -> Self {
        Self { operation, inverse }
    }
}

/// An operation dropped because the thing it addressed no longer exists.
///
/// Distinct from [`Rebase::Satisfied`] on purpose: a tombstone is a **loss**, and the
/// no-silent-loss rule means it must reach the disposition taxonomy (doc 35). A no-op
/// return value would have made every tombstone silent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tombstone {
    /// The operation that was dropped.
    pub operation: &'static str,
    /// The concurrent operation that removed its anchor.
    pub against: &'static str,
}

/// What `subject` becomes on a state where `against` has already been applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rebase {
    /// The operation, in the coordinates the state now has.
    Keep(Operation),
    /// Two or more operations, in application order.
    ///
    /// One intention does not always survive as one operation. A concurrent
    /// [`Operation::SplitParagraph`] inside a range leaves half of it in each paragraph,
    /// and `DeleteText`/`FormatText`/`ClearFormatting` each name a range **within one
    /// paragraph**; a concurrent formatting write over part of a range leaves the earlier
    /// operation holding up to three pieces (§5.8).
    ///
    /// There is no `Batch` operation and adding one is forbidden (ADR-030 I2) — but a
    /// [`Transaction`](crate::Transaction) already carries a sequence, so the multiplicity
    /// lives in the envelope that already had it.
    KeepMany(Vec<Operation>),
    /// Nothing is left for this operation to do, and nothing was lost: the concurrent
    /// change already achieved its intent, or overwrote the same property later.
    Satisfied,
    /// The thing this operation addressed no longer exists. **Must be reported.**
    Tombstoned(Tombstone),
}

impl Rebase {
    /// The rebased operations, in application order — empty when nothing survived.
    ///
    /// The shape a caller builds a [`Transaction`](crate::Transaction) from.
    #[must_use]
    pub fn into_operations(self) -> Vec<Operation> {
        match self {
            Self::Keep(operation) => vec![operation],
            Self::KeepMany(operations) => operations,
            Self::Satisfied | Self::Tombstoned(_) => Vec::new(),
        }
    }
}

/// Why a pair could not be transformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TransformError {
    /// This pair has no answer. The caller must serialise the two operations some other
    /// way — refuse the edit, or make the client reload from a snapshot.
    ///
    /// Deliberately an error and not a best guess: an untransformed operation applied to a
    /// state it was not written against diverges the replicas *quietly*.
    Unsupported {
        /// The operation being transformed.
        subject: &'static str,
        /// The operation it is being transformed against.
        against: &'static str,
        /// Why no answer exists, as a sentence a host can pass through.
        reason: &'static str,
    },
}

impl core::fmt::Display for TransformError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Unsupported {
                subject,
                against,
                reason,
            } => write!(
                formatter,
                "cannot transform {subject} against {against}: {reason}"
            ),
        }
    }
}

impl core::error::Error for TransformError {}

/// Where a block sits — the one fact two operations cannot tell each other.
///
/// Half the block-structure operations name a *position among siblings*
/// (`InsertBlocks`, `DeleteBlocks`, `InsertTable`, `InsertFieldRange`) and
/// `SplitParagraph`/`JoinParagraphs` name a *paragraph*. Neither carries the other's
/// coordinate, and no inverse records it. This is the one place the op set is weaker than
/// ADR-030 I3 promises, and widening the op set to fix it is forbidden by I2 — so the fact
/// is **handed in**, exactly as the sibling engine hands its transform a sheet-name table.
///
/// # The precondition, which is load-bearing
///
/// The answer must describe the state **both operations were written against** — the base.
/// Neither replica holds that state at the moment it transforms: one has applied `a`, the
/// other `b`. A wrong answer here does not error; it silently mis-shifts an index. A
/// session that cannot produce a base-state placement must pass [`NoPlacement`] and take
/// the refusal. Doc 150 §4 and §10 Q1.
pub trait BlockPlacement {
    /// The container holding `node`'s block and `node`'s 0-based index within it, in the
    /// base state. `None` when the placement is unknown, which becomes a refusal rather
    /// than a guess.
    fn block_position(&self, node: NodeId) -> Option<(Option<NodeId>, u32)>;
}

/// A [`BlockPlacement`] that knows nothing, so every pair needing one is refused.
///
/// What plain [`transform`] uses. A caller that can resolve the base state should pass a
/// [`BlockIndex`] to [`transform_placed`] instead and get the pair answered.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPlacement;

impl BlockPlacement for NoPlacement {
    fn block_position(&self, _: NodeId) -> Option<(Option<NodeId>, u32)> {
        None
    }
}

/// Every block's container and sibling index, resolved once.
///
/// **O(document blocks) to build, O(1) to query.** Building one per arriving operation
/// would violate `107` B2; a session that maintains one incrementally alongside the log
/// pays nothing per operation (doc 150 §10 Q1).
///
/// An unwalked container answers `None`, which becomes a **refusal**, never a wrong
/// answer — which is why the seam is safe to ship before every container kind is walked.
#[derive(Clone, Debug, Default)]
pub struct BlockIndex {
    positions: HashMap<NodeId, (Option<NodeId>, u32)>,
}

impl BlockIndex {
    /// Indexes every block of `document`: the body, table cells, block content controls
    /// and text-box bodies.
    #[must_use]
    pub fn of(document: &Document) -> Self {
        let mut positions = HashMap::new();
        index_blocks(document.body(), None, &mut positions);
        Self { positions }
    }
}

impl BlockPlacement for BlockIndex {
    fn block_position(&self, node: NodeId) -> Option<(Option<NodeId>, u32)> {
        self.positions.get(&node).copied()
    }
}

fn index_blocks(
    blocks: &[BlockNode],
    container: Option<NodeId>,
    out: &mut HashMap<NodeId, (Option<NodeId>, u32)>,
) {
    for (index, block) in blocks.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        out.insert(block_id(block), (container, index));
        match block {
            BlockNode::Paragraph(paragraph) => {
                for inline in &paragraph.inlines {
                    index_inline(inline, out);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        index_blocks(&cell.blocks, Some(cell.id), out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => index_blocks(&sdt.blocks, Some(sdt.id), out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

fn index_inline(inline: &InlineNode, out: &mut HashMap<NodeId, (Option<NodeId>, u32)>) {
    match inline {
        InlineNode::Hyperlink(node) => {
            for child in &node.inlines {
                index_inline(child, out);
            }
        }
        InlineNode::Field(node) => {
            for child in &node.inlines {
                index_inline(child, out);
            }
        }
        InlineNode::Revision(node) => {
            for child in &node.inlines {
                index_inline(child, out);
            }
        }
        InlineNode::Sdt(node) => {
            for child in &node.inlines {
                index_inline(child, out);
            }
        }
        InlineNode::TextBox(node) => index_blocks(&node.blocks, Some(node.id), out),
        // Everything else carries no block container. DrawingML group internals are
        // deliberately not walked: a block inside a group answers `None`, which is a
        // refusal, not a wrong answer.
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::Break(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::Group(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_)
        | InlineNode::Math(_)
        | InlineNode::Symbol(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::PositionalTab(_) => {}
    }
}

fn block_id(block: &BlockNode) -> NodeId {
    match block {
        BlockNode::Paragraph(paragraph) => paragraph.id,
        BlockNode::Table(table) => table.id,
        BlockNode::Sdt(sdt) => sdt.id,
        BlockNode::AltChunk(chunk) => chunk.id,
    }
}

/// Rebase `subject` so it can be applied after `against` already has been.
///
/// Both must have been written against the same state. Returns the operation with the same
/// intent expressed in the coordinates the state now has, a pair of operations when a
/// concurrent split divided a range, or a report that nothing is left of it.
///
/// **O(1) in document size**; see the module docs.
///
/// # Errors
///
/// [`TransformError::Unsupported`] for the four shapes in the module docs. A caller must
/// treat that as "these two cannot be merged", never as "carry on".
pub fn transform(
    subject: &Operation,
    against: Change<'_>,
    side: Side,
) -> Result<Rebase, TransformError> {
    transform_placed(subject, against, side, &NoPlacement)
}

/// [`transform`], with the block placement the sibling-indexed pairs need.
///
/// # Errors
///
/// As [`transform`], minus the pairs `placement` can answer. A placement that does not
/// know a node still yields a refusal rather than a guess.
pub fn transform_placed(
    subject: &Operation,
    against: Change<'_>,
    side: Side,
    placement: &dyn BlockPlacement,
) -> Result<Rebase, TransformError> {
    let names = (variant_name(subject), variant_name(against.operation));
    let refuse = |reason: &'static str| TransformError::Unsupported {
        subject: names.0,
        against: names.1,
        reason,
    };
    let effect = effect_of(against).map_err(refuse)?;
    let removed = removed_by(against);

    // 1. A whole-paragraph rewrite meeting a concurrent split or join of that paragraph.
    //    The rewrite replaces content the split carved into a *second* paragraph, and no
    //    operation removes that paragraph as part of the rewrite — so one order keeps the
    //    carved-off half and the other never had it. Three answers are defensible and none
    //    converges, so the pair is refused rather than guessed. Ahead of the liveness step
    //    because a join makes the rewritten paragraph *vanish*, which would otherwise read
    //    as an ordinary tombstone and diverge quietly instead of refusing loudly.
    //
    //    A join is checked on **both** its paragraphs, not just the one that disappears: a
    //    rewrite of `first` and a join that appends `second`'s text to it disagree about
    //    what the paragraph ends up holding, and the rewrite wins in one order and loses a
    //    merge in the other.
    if let Some(structural) = match effect {
        Effect::Split { original, .. } => Some((original, None)),
        Effect::Join { first, second, .. } => Some((first, Some(second))),
        _ => None,
    } {
        let mut rewritten = Vec::new();
        rewritten_nodes(subject, &mut rewritten);
        if rewritten.contains(&structural.0)
            || structural.1.is_some_and(|node| rewritten.contains(&node))
        {
            return Err(refuse(REWRITE_MEETS_A_STRUCTURAL_EDIT));
        }
    }

    // 2. Liveness. `against.inverse` names every node and key that ceased to exist, so
    //    "did the concurrent change destroy what this operation addresses?" is decidable
    //    from the pair alone.
    if let Some(tombstone) = tombstoned(subject, &removed, &effect, names) {
        return Ok(Rebase::Tombstoned(tombstone));
    }

    // 3. Whole-subtree rewrites. `SetInlines`, `UpdateReviewState` and `ReplaceTable`
    //    replace a node's entire content (doc 150 §5.5), and they resolve two ways.
    //
    //    Against **another rewrite** of the same node it is last-writer-wins: the earlier
    //    one is a tombstone, not a no-op, because content was destroyed and the loss is
    //    reportable.
    //
    //    Against a **positional edit inside that paragraph** there is no answer at all, in
    //    either direction. Applying the rewrite first and the edit after puts the typed
    //    text into the replacement; applying the edit first and the rewrite after throws it
    //    away. Yielding either one converges on neither order, because the operation
    //    applied first in the diamond has already landed and cannot be withdrawn — the only
    //    convergent answer is a rewrite that *contains* the other edit, which means
    //    reconstructing its inline list, and a transform that rebuilds an operation's
    //    payload from another's is not a rebase. Refused rather than guessed.
    let mut rewritten = Vec::new();
    rewritten_nodes(against.operation, &mut rewritten);
    rewritten_nodes(subject, &mut rewritten);
    if !rewritten.is_empty() {
        let (mine, theirs) = (rewrites_of(subject), rewrites_of(against.operation));
        if !mine.is_empty() && !theirs.is_empty() {
            if side == Side::Earlier && mine.iter().any(|node| theirs.contains(node)) {
                return Ok(Rebase::Tombstoned(Tombstone {
                    operation: names.0,
                    against: names.1,
                }));
            }
        } else if rewritten
            .iter()
            .any(|node| positional_in(subject, *node) || positional_in(against.operation, *node))
        {
            return Err(refuse(REWRITE_MEETS_A_POSITIONAL_EDIT));
        }
    }

    // 4. Positional rebase.
    let rebased = match rebase_coordinates(subject, &effect, side, placement) {
        Ok(rebased) => rebased,
        Err(reason) => return Err(refuse(reason)),
    };
    let subject = match rebased {
        CoordinateRebase::Moved(coordinates) => {
            let mut moved = subject.clone();
            set_coordinates(&mut moved, coordinates);
            moved
        }
        CoordinateRebase::Divided(first, second) => {
            return divide(subject, first, second).ok_or_else(|| refuse(HYPERLINK_CANNOT_DIVIDE));
        }
        CoordinateRebase::Satisfied => return Ok(Rebase::Satisfied),
        CoordinateRebase::Tombstoned => {
            return Ok(Rebase::Tombstoned(Tombstone {
                operation: names.0,
                against: names.1,
            }));
        }
    };

    // 5. The table grid is shared state on TWO axes, and an operation that carries a row
    //    or a column carries one cell per line of the other axis. `107` §8 Q4 guessed this
    //    was where TP1 would fail; it is, but not for the reason it gave — the index
    //    arithmetic converges, and what does not is the **payload**.
    let mut subject = subject;
    if let Some(fixed) = rebase_table_payload(&subject, &effect) {
        subject = fixed.map_err(refuse)?;
    }

    // 6. Contention. Two writes to the same target resolve in the settled order, per
    //    aspect: the earlier yields exactly what the later overwrites and nothing else.
    if let Some(rebased) = contend_over_text(&subject, against.operation, side) {
        return Ok(rebased);
    }
    Ok(contend(subject, against.operation, side))
}

/// Four of the matches over [`Operation`]: `tier`, `variant_name`, `coordinates` and its
/// mirror `set_coordinates`. The other three — `anchors`, `anchor_key` and `footprint` —
/// sit beside the steps that read them.
mod classify;
/// What a committed change did to the coordinate spaces other operations address, read
/// from the operation and the inverse together.
mod effect;

use classify::{Container, Coordinates, coordinates, set_coordinates};
pub use classify::{Tier, tier, variant_name};
use effect::{Band, Effect, Key, Removed, TextEffect, effect_of, removed_by};
#[cfg(test)]
use effect::{collect_block, collect_inline};

// ---------------------------------------------------------------------------------------
// Step 1: liveness.
// ---------------------------------------------------------------------------------------

/// The nodes `operation` needs to still exist.
///
/// Exhaustive by construction. O(1), except `UpdateReviewState`, which names one node per
/// paragraph it rewrites.
fn anchors(operation: &Operation, out: &mut Vec<NodeId>) {
    match operation {
        Operation::InsertText { at, .. }
        | Operation::SplitParagraph { at, .. }
        | Operation::InsertInlineObject { at, .. }
        | Operation::InsertField { at, .. }
        | Operation::InsertNote { at, .. } => out.push(at.node),
        Operation::DeleteText { range }
        | Operation::FormatText { range, .. }
        | Operation::ClearFormatting { range }
        | Operation::SetHyperlink { range, .. } => out.push(range.start.node),
        Operation::CreateBookmark { start, end, .. } => {
            out.push(start.node);
            out.push(end.node);
        }
        Operation::JoinParagraphs { first, second, .. } => {
            out.push(*first);
            out.push(*second);
        }
        Operation::SetInlines { node, .. } | Operation::SetParagraphProperties { node, .. } => {
            out.push(*node);
        }
        Operation::InsertRow { table, .. }
        | Operation::DeleteRow { table, .. }
        | Operation::InsertColumn { table, .. }
        | Operation::DeleteColumn { table, .. }
        | Operation::DeleteTable { table }
        | Operation::SetTableProperties { table, .. }
        | Operation::ReplaceTable { table, .. } => out.push(*table),
        Operation::InsertTable { container, .. }
        | Operation::InsertBlocks { container, .. }
        | Operation::DeleteBlocks { container, .. } => out.extend(*container),
        Operation::SetExtent { object, .. }
        | Operation::SetGroupGeometry { object, .. }
        | Operation::SetAnchor { object, .. }
        | Operation::SetImageCrop { object, .. }
        | Operation::SetObjectDescr { object, .. }
        | Operation::DeleteObject { object }
        | Operation::RemoveInlineObject { object }
        | Operation::SetTextBoxBody { object, .. } => out.push(*object),
        Operation::InsertObjectNode { owner, .. } => out.push(*owner),
        Operation::SetTableCellProperties { cell, .. } => out.push(*cell),
        Operation::UpdateReviewState { paragraphs, .. } => {
            out.extend(paragraphs.iter().map(|state| state.node));
        }
        Operation::RemoveField { field } => out.push(*field),
        Operation::RemoveNote { reference_id, .. } => out.push(*reference_id),
        Operation::SetShapeFill { shape, .. } | Operation::SetShapeStroke { shape, .. } => {
            out.push(*shape);
        }
        // Registry- and document-scoped: no node anchor. Their liveness is `anchor_key`.
        Operation::SetCoreProperties { .. }
        | Operation::SetSectionGeometry { .. }
        | Operation::SpliceSectionBoundary { .. }
        | Operation::SetStyleDefinition { .. }
        | Operation::SetAbstractNumbering { .. }
        | Operation::SetNumberingInstance { .. }
        | Operation::SetMediaReference { .. }
        | Operation::DeleteBookmark { .. }
        | Operation::RenameBookmark { .. }
        | Operation::InsertFieldRange { .. }
        | Operation::RemoveFieldRange { .. }
        | Operation::CreateHeaderFooterBody { .. }
        | Operation::RemoveHeaderFooterBody { .. }
        | Operation::SetSectionRunningRef { .. }
        | Operation::SetSectionTitlePage { .. }
        | Operation::SetSectionWatermark { .. }
        | Operation::SetSectionLineNumbering { .. }
        | Operation::SetSectionPageNumbering { .. }
        | Operation::SetSectionVerticalAlignment { .. }
        | Operation::SetEvenAndOddHeaders { .. } => {}
    }
}

/// The registry key `operation` needs to still exist, when it has one.
///
/// Exhaustive by construction. One key per operation: an operation that depends on two
/// (a `SetSectionRunningRef` naming both a section and a body) reports the one whose
/// removal is the likelier race, and the other is caught by `apply` refusing. O(1).
fn anchor_key(operation: &Operation) -> Option<Key> {
    match operation {
        Operation::DeleteBookmark { bookmark } | Operation::RenameBookmark { bookmark, .. } => {
            Some(Key::Bookmark(*bookmark))
        }
        Operation::RemoveFieldRange { field } => Some(Key::FieldRange(*field)),
        Operation::RemoveNote { kind, note, .. } => Some(Key::Note(*kind, *note)),
        Operation::RemoveHeaderFooterBody { region, id } => Some(Key::HeaderFooter(*region, *id)),
        Operation::SetSectionGeometry { section, .. }
        | Operation::SetSectionRunningRef { section, .. }
        | Operation::SetSectionTitlePage { section, .. }
        | Operation::SetSectionWatermark { section, .. }
        | Operation::SetSectionLineNumbering { section, .. }
        | Operation::SetSectionPageNumbering { section, .. }
        | Operation::SetSectionVerticalAlignment { section, .. } => Some(Key::Section(*section)),
        Operation::SpliceSectionBoundary { at, .. } => at.map(Key::Section),
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
        | Operation::UpdateReviewState { .. }
        | Operation::SetStyleDefinition { .. }
        | Operation::SetAbstractNumbering { .. }
        | Operation::SetNumberingInstance { .. }
        | Operation::SetMediaReference { .. }
        | Operation::CreateBookmark { .. }
        | Operation::InsertField { .. }
        | Operation::RemoveField { .. }
        | Operation::InsertFieldRange { .. }
        | Operation::InsertNote { .. }
        | Operation::CreateHeaderFooterBody { .. }
        | Operation::SetEvenAndOddHeaders { .. }
        | Operation::SetShapeFill { .. }
        | Operation::SetShapeStroke { .. }
        | Operation::SetTextBoxBody { .. } => None,
    }
}

fn tombstoned(
    subject: &Operation,
    removed: &Removed,
    effect: &Effect,
    names: (&'static str, &'static str),
) -> Option<Tombstone> {
    // A join meeting another join is not an anchor death: the paragraphs merged, which is
    // what this join wanted, and `rebase_coordinates` decides whether it is satisfied or
    // follows the merge. A join meeting anything else that removed one of its paragraphs
    // IS a tombstone.
    if matches!(subject, Operation::JoinParagraphs { .. }) && matches!(effect, Effect::Join { .. })
    {
        return None;
    }
    // A join **relocates** the text of `second` into `first`; it does not destroy the
    // positions inside it. So a position-addressed operation follows its text rather than
    // being tombstoned — which is the whole point of anchoring to nodes and offsets rather
    // than to a document-wide coordinate. An operation addressed to `second` as a *node*
    // (its paragraph properties, say) really has lost its target, and still tombstones.
    if matches!(effect, Effect::Join { .. })
        && matches!(
            coordinates(subject),
            Coordinates::Caret(_) | Coordinates::Range(_) | Coordinates::Carets(..)
        )
    {
        return None;
    }
    let mut nodes = Vec::new();
    anchors(subject, &mut nodes);
    if nodes.iter().any(|node| removed.holds(*node))
        || anchor_key(subject).is_some_and(|key| removed.holds_key(key))
    {
        return Some(Tombstone {
            operation: names.0,
            against: names.1,
        });
    }
    None
}

/// The nodes whose entire content `operation` replaces.
fn rewritten_nodes(operation: &Operation, out: &mut Vec<NodeId>) {
    match operation {
        Operation::SetInlines { node, .. } => out.push(*node),
        Operation::ReplaceTable { table, .. } => out.push(*table),
        Operation::UpdateReviewState { paragraphs, .. } => {
            out.extend(paragraphs.iter().map(|state| state.node));
        }
        _ => {}
    }
}

/// Adjusts the cells a row or column insertion carries when the concurrent change moved
/// the *other* axis of the same table.
///
/// `InsertRow` carries one cell per grid column and `InsertColumn` carries one cell per
/// row, so a concurrent change to the other axis leaves the payload the wrong shape — and
/// `casual_doc_edit::apply` only refuses an *irregular* table, so the wrong shape lands as
/// a table one replica has and the other does not. This is the real content of `107`
/// §8 Q4: the grid is shared state on two axes, the index arithmetic converges, and the
/// payload is what needs rebasing.
///
/// A concurrent **removal** on the other axis drops the matching cell, which is exact. A
/// concurrent **insertion** needs one more cell than the operation carries, and a cell
/// needs fresh identities for itself and the paragraph inside it — which a pure transform
/// may not mint. Refused (doc 150 §6 U9).
///
/// Returns `None` when this is not such a pair. O(cells the operation carries).
fn rebase_table_payload(
    subject: &Operation,
    effect: &Effect,
) -> Option<Result<Operation, &'static str>> {
    let Effect::Band(band) = *effect else {
        return None;
    };
    let mut rebased = subject.clone();
    match (&mut rebased, band.container) {
        (Operation::InsertRow { table, row, .. }, Container::GridColumns(banded))
            if *table == banded =>
        {
            if band.inserting {
                return Some(Err(TABLE_AXIS_NEEDS_A_FRESH_CELL));
            }
            let at = band.at as usize;
            if at >= row.cells.len() {
                return None;
            }
            row.cells.remove(at);
        }
        (Operation::InsertColumn { table, cells, .. }, Container::Rows(banded))
            if *table == banded =>
        {
            if band.inserting {
                return Some(Err(TABLE_AXIS_NEEDS_A_FRESH_CELL));
            }
            let at = band.at as usize;
            if at >= cells.len() {
                return None;
            }
            cells.remove(at);
        }
        _ => return None,
    }
    Some(Ok(rebased))
}

/// The nodes `operation` rewrites, as a list.
fn rewrites_of(operation: &Operation) -> Vec<NodeId> {
    let mut out = Vec::new();
    rewritten_nodes(operation, &mut out);
    out
}

/// Whether `operation` addresses a **position inside** `node`'s content.
///
/// The question a whole-paragraph rewrite has to ask about a concurrent operation. A write
/// to the paragraph's own *properties* is not positional and commutes with a rewrite of its
/// inlines, so it is deliberately excluded: making every operation that merely names the
/// paragraph count would refuse pairs that have a perfectly good answer.
fn positional_in(operation: &Operation, node: NodeId) -> bool {
    match coordinates(operation) {
        Coordinates::Caret(at) => at.node == node,
        Coordinates::Range(range) => range.start.node == node,
        Coordinates::Carets(start, end) => start.node == node || end.node == node,
        Coordinates::Adjacent(first, second) => first == node || second == node,
        Coordinates::Slot { container, .. } => container == Container::Inlines(node),
        Coordinates::None => false,
    }
}

// ---------------------------------------------------------------------------------------
// Step 3: the positional rebase.
// ---------------------------------------------------------------------------------------

enum CoordinateRebase {
    Moved(Coordinates),
    /// A concurrent change divided a range in two, **in application order**.
    Divided(EditRange, EditRange),
    Satisfied,
    Tombstoned,
}

/// What a range operation does to the text it names, which decides how it reads an
/// insertion at its own boundaries.
///
/// The two are genuinely different rules and the difference is not cosmetic:
///
/// - A **removal** must not take text somebody typed inside it concurrently. That is the
///   intention the other author expressed, and it is the one `apply` reproduces in the
///   other order — so the removal splits in two around the insertion.
/// - A **span** must cover it. A bold applied over `abc` while somebody types in the
///   middle bolds the typed text too, because in the other order the typing lands inside
///   an already-bold run and inherits its properties. Widening the span is what makes the
///   two orders agree, and it is also what Word does.
///
/// Getting this backwards is not a subtle wrong answer — it is TP1 failing on the two most
/// common operations in the set, which is how the rule was established rather than
/// assumed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RangeKind {
    /// The range's text is removed.
    Removes,
    /// The range is a span over text that stays.
    Spans,
}

const fn range_kind(operation: &Operation) -> RangeKind {
    match operation {
        Operation::DeleteText { .. } => RangeKind::Removes,
        _ => RangeKind::Spans,
    }
}

/// Rebases a same-paragraph range through `effect`.
fn map_range(range: EditRange, kind: RangeKind, effect: &Effect) -> CoordinateRebase {
    let node = range.start.node;
    let (start, end) = (range.start.offset, range.end.offset);
    let whole = |start: u32, end: u32| {
        CoordinateRebase::Moved(Coordinates::Range(EditRange {
            start: Pos::new(node, start),
            end: Pos::new(node, end),
        }))
    };
    match *effect {
        Effect::Text(TextEffect::Insert {
            node: at_node,
            at,
            len,
        }) if at_node == node => match kind {
            RangeKind::Removes => {
                if at <= start {
                    whole(start.saturating_add(len), end.saturating_add(len))
                } else if at >= end {
                    whole(start, end)
                } else {
                    // The insertion sits inside the span this operation removes. Taking it
                    // would destroy the other author's text, so the removal divides around
                    // it — and the **later** half is applied first, because removing the
                    // earlier half would move the later one's offsets out from under it.
                    CoordinateRebase::Divided(
                        EditRange {
                            start: Pos::new(node, at.saturating_add(len)),
                            end: Pos::new(node, end.saturating_add(len)),
                        },
                        EditRange {
                            start: Pos::new(node, start),
                            end: Pos::new(node, at),
                        },
                    )
                }
            }
            // A span's boundaries are decided by where `apply` ATTACHES inserted text,
            // which is "the first run whose `[start, end]` contains the offset". So an
            // insertion at an interior boundary joins the run *before* it and falls
            // outside a span starting there, while an insertion at offset 0 has no run
            // before it, joins the first run, and falls *inside* a span starting at 0.
            //
            // This is the "one normal form" rule with teeth: the span arithmetic is not
            // free to be prettier than `casual_doc_edit::insert_text`, and
            // `a_span_follows_the_run_an_insertion_attaches_to` pins the dependency so a
            // change to that attachment rule fails here rather than diverging silently.
            RangeKind::Spans => {
                let start_shifts = if at == 0 { start > 0 } else { start >= at };
                whole(
                    if start_shifts {
                        start.saturating_add(len)
                    } else {
                        start
                    },
                    if end >= at {
                        end.saturating_add(len)
                    } else {
                        end
                    },
                )
            }
        },
        Effect::Text(TextEffect::Delete {
            node: at_node,
            start: removed_start,
            end: removed_end,
        }) if at_node == node => {
            let map = |offset: u32| {
                if offset <= removed_start {
                    offset
                } else if offset >= removed_end {
                    offset - (removed_end - removed_start)
                } else {
                    removed_start
                }
            };
            let (start, end) = (map(start), map(end));
            if start < end {
                whole(start, end)
            } else {
                // The text this range addressed is gone. Nothing was lost by *this*
                // operation — the other one removed the text — so this is `Satisfied`, not
                // a tombstone. Doc 150 §5.4, answering `107` §8 Q1.
                CoordinateRebase::Satisfied
            }
        }
        Effect::Split {
            original,
            new_node,
            at,
        } if original == node => {
            if at <= start {
                CoordinateRebase::Moved(Coordinates::Range(EditRange {
                    start: Pos::new(new_node, start - at),
                    end: Pos::new(new_node, end - at),
                }))
            } else if at >= end {
                whole(start, end)
            } else {
                // A range names ONE paragraph and the split put half of it in another, so
                // the caller gets two operations. The two are in different paragraphs, so
                // neither order moves the other's offsets.
                CoordinateRebase::Divided(
                    EditRange {
                        start: Pos::new(original, start),
                        end: Pos::new(original, at),
                    },
                    EditRange {
                        start: Pos::new(new_node, 0),
                        end: Pos::new(new_node, end - at),
                    },
                )
            }
        }
        Effect::Join {
            first,
            second,
            at: boundary,
        } if second == node => CoordinateRebase::Moved(Coordinates::Range(EditRange {
            start: Pos::new(first, boundary.saturating_add(start)),
            end: Pos::new(first, boundary.saturating_add(end)),
        })),
        _ => whole(start, end),
    }
}

/// Where a byte offset in `node` ends up after `effect`, and in which paragraph.
///
/// **The tie-break, stated once:** at an exact boundary the operation the server ordered
/// [`Side::Later`] moves, and the [`Side::Earlier`] one holds the position it asked for.
/// Both replicas compute the same answer because both are told the same order.
///
/// This is not the caret rule. `Affinity` decides where *your caret* lands at a boundary;
/// `Side` decides where *content* lands. They agree in the case that matters and answer
/// different questions (doc 150 §5.1, correcting `107` §3.3).
fn map_position(position: Pos, effect: &Effect, side: Side) -> Option<Pos> {
    match *effect {
        Effect::Text(TextEffect::Insert { node, at, len }) if position.node == node => {
            let shifts = position.offset > at || (position.offset == at && side == Side::Later);
            Some(Pos::new(
                node,
                if shifts {
                    position.offset.saturating_add(len)
                } else {
                    position.offset
                },
            ))
        }
        Effect::Text(TextEffect::Delete { node, start, end }) if position.node == node => {
            let offset = if position.offset <= start {
                position.offset
            } else if position.offset >= end {
                position.offset - (end - start)
            } else {
                // Inside the removed span: the text this position was relative to is gone,
                // so the intention collapses onto where it used to begin.
                start
            };
            Some(Pos::new(node, offset))
        }
        Effect::Split {
            original,
            new_node,
            at,
        } if position.node == original => {
            let moves = position.offset > at || (position.offset == at && side == Side::Later);
            Some(if moves {
                Pos::new(new_node, position.offset - at)
            } else {
                Pos::new(original, position.offset)
            })
        }
        Effect::Join { first, second, at } if position.node == second => {
            Some(Pos::new(first, at.saturating_add(position.offset)))
        }
        _ => Some(position),
    }
}

/// Where an insertion **gap** at `index` ends up after `band`, or `None` when the band
/// swallowed the position it was inserting at.
///
/// A gap and an element move differently at the boundary, and conflating them is the
/// classic OT bug: inserting at the same index as another insert still means "before
/// whatever is there now", so it shifts — but *which* of two concurrent inserts ends up
/// first is an observable fact about identity, so the tie is settled by `side`.
const fn shift_gap(index: u32, band: Band, side: Side) -> Option<u32> {
    if band.inserting {
        let shifts = index > band.at || (index == band.at && matches!(side, Side::Later));
        return Some(if shifts { index + band.count } else { index });
    }
    let end = band.at.saturating_add(band.count);
    if index > band.at && index < end {
        // Strictly inside a removed span: the gap no longer exists. `shift_span` widens the
        // matching delete to cover whatever was inserted here, and the two rules are only
        // convergent together.
        None
    } else if index >= end {
        Some(index - band.count)
    } else {
        Some(index)
    }
}

/// Where a span of `count` elements starting at `index` ends up after `band`, or `None`
/// when nothing is left of it.
const fn shift_span(index: u32, count: u32, band: Band) -> Option<(u32, u32)> {
    let end = index.saturating_add(count);
    let band_end = band.at.saturating_add(band.count);
    if band.inserting {
        if band.at <= index {
            return Some((index + band.count, count));
        }
        if band.at >= end {
            return Some((index, count));
        }
        // Strictly inside: the inserted children sit within the span this operation means
        // to remove, so the removal widens to take them. `shift_gap` turns that insertion
        // into nothing; the pair is only convergent together.
        return Some((index, count + band.count));
    }
    let overlap = min(end, band_end).saturating_sub(max(index, band.at));
    let remaining = count.saturating_sub(overlap);
    if remaining == 0 {
        return None;
    }
    let removed_before = min(band_end, index).saturating_sub(band.at);
    Some((index - removed_before, remaining))
}

const fn min(a: u32, b: u32) -> u32 {
    if a < b { a } else { b }
}

const fn max(a: u32, b: u32) -> u32 {
    if a > b { a } else { b }
}

/// Rebases a block slot across a concurrent paragraph split.
///
/// A split is **not** an ordinary insertion at the gap after `original`, and treating it as
/// one breaks TP1 on the commonest pair there is. The new paragraph is *carved out of* a
/// block, so a removal that takes the block must take what was carved out of it too: a
/// delete of `[i, i+c)` covering the split paragraph widens to `[i, i+c+1)`, where an
/// ordinary insertion at the same boundary would not. In the other order the split is
/// tombstoned — its paragraph is gone — and the two agree only because the removal widened.
///
/// An insertion *gap* needs no tie-break: the new paragraph lands immediately after
/// `original`, so a gap strictly after it shifts and a gap at or before it does not, and
/// both orders compute that without consulting the settled order.
fn rebase_slot_across_split(
    container: Container,
    index: u32,
    count: u32,
    original: NodeId,
    placement: &dyn BlockPlacement,
) -> Result<Coordinates, &'static str> {
    let (split_container, at) = placement
        .block_position(original)
        .ok_or(NO_PLACEMENT_FOR_SPLIT)?;
    if Container::Blocks(split_container) != container {
        return Ok(Coordinates::Slot {
            container,
            index,
            count,
        });
    }
    let (index, count) = if count == 0 {
        (if index > at { index + 1 } else { index }, 0)
    } else if at < index {
        (index + 1, count)
    } else if at < index.saturating_add(count) {
        (index, count + 1)
    } else {
        (index, count)
    };
    Ok(Coordinates::Slot {
        container,
        index,
        count,
    })
}

/// Rebases a block slot across a concurrent paragraph join.
///
/// The mirror of the split, and the one case that has **no** answer: a removal whose span
/// covers either paragraph of the join. After the join, `second`'s text lives inside
/// `first`, so removing `first` removes text the other order leaves standing — and no
/// operation in the set expresses "remove the bytes that came from `second`". Three answers
/// are defensible and none converges, so the pair is refused rather than guessed.
fn rebase_slot_across_join(
    container: Container,
    index: u32,
    count: u32,
    second: NodeId,
    placement: &dyn BlockPlacement,
) -> Result<Coordinates, &'static str> {
    let (join_container, at) = placement
        .block_position(second)
        .ok_or(NO_PLACEMENT_FOR_JOIN)?;
    if Container::Blocks(join_container) != container {
        return Ok(Coordinates::Slot {
            container,
            index,
            count,
        });
    }
    let (index, count) = if count == 0 {
        (if index > at { index - 1 } else { index }, 0)
    } else {
        // The two joined paragraphs occupy `[at - 1, at + 1)`.
        let end = index.saturating_add(count);
        let overlaps = index < at.saturating_add(1) && at <= end;
        if overlaps {
            return Err(JOIN_MERGED_INTO_A_REMOVAL);
        }
        if at < index {
            (index - 1, count)
        } else {
            (index, count)
        }
    };
    Ok(Coordinates::Slot {
        container,
        index,
        count,
    })
}

#[allow(clippy::too_many_lines)]
fn rebase_coordinates(
    subject: &Operation,
    effect: &Effect,
    side: Side,
    placement: &dyn BlockPlacement,
) -> Result<CoordinateRebase, &'static str> {
    Ok(match coordinates(subject) {
        Coordinates::None => CoordinateRebase::Moved(Coordinates::None),

        Coordinates::Caret(at) => {
            // **U5.** A concurrent join lands offset 0 of `second` exactly on the run
            // boundary the join created, and `casual_doc_edit::insert_text` attaches text
            // at a boundary to the run BEFORE it. So in one order the characters join the
            // text that was already in `first`, and in the other they join the text that
            // came from `second` — the same string, two different run partitions, and two
            // different sets of run properties the moment those runs differ.
            //
            // `Pos` carries no affinity, so the operation cannot say which side of the
            // boundary it meant. `Affinity` exists one layer up on
            // [`Position`](crate::Position) and putting it on `Pos` is an op-set change
            // (ADR-030 I2), so this is recorded as a finding and refused, not guessed.
            if let Effect::Join { second, .. } = *effect
                && at.node == second
                && at.offset == 0
                && matches!(
                    subject,
                    Operation::InsertText { .. } | Operation::InsertField { .. }
                )
            {
                return Err(INSERTION_AT_AN_ABSORBED_PARAGRAPH_START);
            }
            match map_position(at, effect, side) {
                Some(moved) => CoordinateRebase::Moved(Coordinates::Caret(moved)),
                None => CoordinateRebase::Tombstoned,
            }
        }

        Coordinates::Carets(start, end) => {
            match (
                map_position(start, effect, side),
                map_position(end, effect, side),
            ) {
                (Some(start), Some(end)) => {
                    CoordinateRebase::Moved(Coordinates::Carets(start, end))
                }
                _ => CoordinateRebase::Tombstoned,
            }
        }

        Coordinates::Range(range) => map_range(range, range_kind(subject), effect),

        Coordinates::Adjacent(first, second) => match *effect {
            // Somebody else already merged these two. The intention is satisfied.
            Effect::Join {
                first: joined_first,
                second: joined_second,
                ..
            } if joined_first == first && joined_second == second => CoordinateRebase::Satisfied,
            // The paragraph this join was going to absorb was itself absorbed into an
            // earlier one; follow it.
            Effect::Join {
                first: joined_first,
                second: joined_second,
                ..
            } if joined_second == first => {
                CoordinateRebase::Moved(Coordinates::Adjacent(joined_first, second))
            }
            // A split inserted a new paragraph between them, or between `second` and what
            // follows. The first case breaks the adjacency the join requires.
            Effect::Split {
                original, new_node, ..
            } if original == first => {
                if new_node == second {
                    CoordinateRebase::Moved(Coordinates::Adjacent(first, second))
                } else {
                    // `first` now ends where the split left it and `new_node` sits between
                    // the two. Joining `first` to `second` is no longer a join of adjacent
                    // paragraphs, and no operation expresses "join across a third".
                    return Err(SPLIT_SEPARATED_THE_JOIN);
                }
            }
            Effect::Band(band) if matches!(band.container, Container::Blocks(_)) => {
                let (container, index) = match placement.block_position(second) {
                    Some(position) => position,
                    None => {
                        return Err(NO_PLACEMENT_FOR_ADJACENCY);
                    }
                };
                if band.container != Container::Blocks(container) {
                    CoordinateRebase::Moved(Coordinates::Adjacent(first, second))
                } else if band.inserting && band.at == index && band.count > 0 {
                    return Err(INSERTION_SEPARATED_THE_JOIN);
                } else {
                    CoordinateRebase::Moved(Coordinates::Adjacent(first, second))
                }
            }
            _ => CoordinateRebase::Moved(Coordinates::Adjacent(first, second)),
        },

        Coordinates::Slot {
            container,
            index,
            count,
        } => {
            // A split or a join changes a block container's membership at a position only
            // a placement can supply. Every other container is untouched by them.
            let band = match *effect {
                Effect::Split { original, .. } => {
                    if matches!(container, Container::Blocks(_)) {
                        return Ok(CoordinateRebase::Moved(rebase_slot_across_split(
                            container, index, count, original, placement,
                        )?));
                    }
                    None
                }
                Effect::Join { second, .. } => {
                    if matches!(container, Container::Blocks(_)) {
                        return Ok(CoordinateRebase::Moved(rebase_slot_across_join(
                            container, index, count, second, placement,
                        )?));
                    }
                    None
                }
                Effect::Band(band) => Some(band),
                // An inline-index operation cannot be rebased across a membership change
                // expressed as a byte offset: the index the run splitting produced is on
                // neither operation (refusal U4).
                Effect::Text(TextEffect::Insert { node, .. } | TextEffect::Delete { node, .. }) => {
                    if container == Container::Inlines(node) {
                        return Err(INLINE_INDEX_ACROSS_TEXT);
                    }
                    None
                }
                Effect::InlineMembership { first, second } => {
                    if container == Container::Inlines(first)
                        || second.is_some_and(|node| container == Container::Inlines(node))
                    {
                        return Err(INLINE_INDEX_ACROSS_MEMBERSHIP);
                    }
                    None
                }
                Effect::Inert => None,
            };
            let Some(band) = band else {
                return Ok(CoordinateRebase::Moved(Coordinates::Slot {
                    container,
                    index,
                    count,
                }));
            };
            if band.container != container {
                return Ok(CoordinateRebase::Moved(Coordinates::Slot {
                    container,
                    index,
                    count,
                }));
            }
            if count == 0 {
                match shift_gap(index, band, side) {
                    Some(index) => CoordinateRebase::Moved(Coordinates::Slot {
                        container,
                        index,
                        count,
                    }),
                    None => CoordinateRebase::Satisfied,
                }
            } else {
                match shift_span(index, count, band) {
                    Some((index, count)) => CoordinateRebase::Moved(Coordinates::Slot {
                        container,
                        index,
                        count,
                    }),
                    None => CoordinateRebase::Satisfied,
                }
            }
        }
    })
}

/// Builds the two operations a divided range needs, or `None` when the operation cannot be
/// expressed twice.
fn divide(subject: &Operation, first: EditRange, second: EditRange) -> Option<Rebase> {
    match subject {
        Operation::DeleteText { .. } => Some(Rebase::KeepMany(vec![
            Operation::DeleteText { range: first },
            Operation::DeleteText { range: second },
        ])),
        Operation::FormatText { delta, .. } => Some(Rebase::KeepMany(vec![
            Operation::FormatText {
                range: first,
                delta: delta.clone(),
            },
            Operation::FormatText {
                range: second,
                delta: delta.clone(),
            },
        ])),
        Operation::ClearFormatting { .. } => Some(Rebase::KeepMany(vec![
            Operation::ClearFormatting { range: first },
            Operation::ClearFormatting { range: second },
        ])),
        // U1: two wrappers need two fresh node ids and the operation carries one. Minting
        // one inside a pure function, or reusing the one it has, are both wrong.
        Operation::SetHyperlink { .. } => None,
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------
// Step 4: contention.
// ---------------------------------------------------------------------------------------

/// The thing a retained-value write claims.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Node(NodeId),
    Bookmark(BookmarkId),
    Style(StyleId),
    AbstractNumbering(AbstractNumberingId),
    NumberingInstance(NumberingInstanceId),
    Media(MediaId),
    Section(SectionId),
    SectionRunning(SectionId, RunningRegion, HeaderFooterKind),
    CoreProperties,
    Settings,
}

/// Which independent field a write claims. The earlier operation yields exactly the
/// aspects the later one overwrites, and nothing else — a concurrent resize and a
/// concurrent recolour of one shape must not destroy each other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Aspects(u32);

impl Aspects {
    const PARAGRAPH_PROPERTIES: Self = Self(1 << 0);
    const EXTENT: Self = Self(1 << 1);
    const GROUP_TRANSFORM: Self = Self(1 << 2);
    const ANCHOR: Self = Self(1 << 3);
    const CROP: Self = Self(1 << 4);
    const DESCR: Self = Self(1 << 5);
    const CELL_PROPERTIES: Self = Self(1 << 6);
    const TABLE_PROPERTIES: Self = Self(1 << 7);
    const FILL: Self = Self(1 << 8);
    const STROKE: Self = Self(1 << 9);
    const TEXT_BOX_BODY: Self = Self(1 << 10);
    const CORE_PROPERTIES: Self = Self(1 << 11);
    const STYLE: Self = Self(1 << 12);
    const BOOKMARK_NAME: Self = Self(1 << 13);
    const SECTION_GEOMETRY: Self = Self(1 << 14);
    const SECTION_RUNNING_REF: Self = Self(1 << 15);
    const SECTION_TITLE_PAGE: Self = Self(1 << 16);
    const SECTION_WATERMARK: Self = Self(1 << 17);
    const SECTION_LINE_NUMBERING: Self = Self(1 << 18);
    const SECTION_PAGE_NUMBERING: Self = Self(1 << 19);
    const SECTION_VERTICAL_ALIGNMENT: Self = Self(1 << 20);
    const EVEN_AND_ODD_HEADERS: Self = Self(1 << 21);
    const NUMBERING_DEFINITION: Self = Self(1 << 22);
    const MEDIA_REFERENCE: Self = Self(1 << 23);

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

/// What a retained-value write claims, or `None` for everything resolved positionally.
///
/// Exhaustive by construction. Contention applies **only** to the self-inverse `Set*`
/// operations: a text edit and a band are resolved by arithmetic and both stand, and a
/// whole-subtree rewrite is resolved by `rewrites_anchor_of`. O(1).
fn footprint(operation: &Operation) -> Option<(Target, Aspects)> {
    match operation {
        Operation::SetParagraphProperties { node, .. } => {
            Some((Target::Node(*node), Aspects::PARAGRAPH_PROPERTIES))
        }
        Operation::SetExtent { object, .. } => Some((Target::Node(*object), Aspects::EXTENT)),
        Operation::SetGroupGeometry { object, .. } => Some((
            Target::Node(*object),
            Aspects::EXTENT.union(Aspects::GROUP_TRANSFORM),
        )),
        Operation::SetAnchor { object, .. } => Some((Target::Node(*object), Aspects::ANCHOR)),
        Operation::SetImageCrop { object, .. } => Some((Target::Node(*object), Aspects::CROP)),
        Operation::SetObjectDescr { object, .. } => Some((Target::Node(*object), Aspects::DESCR)),
        Operation::SetTableCellProperties { cell, .. } => {
            Some((Target::Node(*cell), Aspects::CELL_PROPERTIES))
        }
        Operation::SetTableProperties { table, .. } => {
            Some((Target::Node(*table), Aspects::TABLE_PROPERTIES))
        }
        Operation::SetShapeFill { shape, .. } => Some((Target::Node(*shape), Aspects::FILL)),
        Operation::SetShapeStroke { shape, .. } => Some((Target::Node(*shape), Aspects::STROKE)),
        Operation::SetTextBoxBody { object, .. } => {
            Some((Target::Node(*object), Aspects::TEXT_BOX_BODY))
        }
        Operation::SetCoreProperties { .. } => {
            Some((Target::CoreProperties, Aspects::CORE_PROPERTIES))
        }
        Operation::SetStyleDefinition { id, .. } => Some((Target::Style(*id), Aspects::STYLE)),
        // One aspect per definition kind. The target already discriminates WHICH definition,
        // so the aspect only has to say that the whole definition is the contested field —
        // there are no independent sub-fields to preserve the way a shape's fill and its
        // extent are independent.
        Operation::SetAbstractNumbering { id, .. } => Some((
            Target::AbstractNumbering(*id),
            Aspects::NUMBERING_DEFINITION,
        )),
        Operation::SetNumberingInstance { id, .. } => Some((
            Target::NumberingInstance(*id),
            Aspects::NUMBERING_DEFINITION,
        )),
        Operation::SetMediaReference { id, .. } => {
            Some((Target::Media(*id), Aspects::MEDIA_REFERENCE))
        }
        Operation::RenameBookmark { bookmark, .. } => {
            Some((Target::Bookmark(*bookmark), Aspects::BOOKMARK_NAME))
        }
        Operation::SetSectionGeometry { section, .. } => {
            Some((Target::Section(*section), Aspects::SECTION_GEOMETRY))
        }
        Operation::SetSectionRunningRef {
            section,
            region,
            kind,
            ..
        } => Some((
            Target::SectionRunning(*section, *region, *kind),
            Aspects::SECTION_RUNNING_REF,
        )),
        Operation::SetSectionTitlePage { section, .. } => {
            Some((Target::Section(*section), Aspects::SECTION_TITLE_PAGE))
        }
        Operation::SetSectionWatermark { section, .. } => {
            Some((Target::Section(*section), Aspects::SECTION_WATERMARK))
        }
        Operation::SetSectionLineNumbering { section, .. } => {
            Some((Target::Section(*section), Aspects::SECTION_LINE_NUMBERING))
        }
        Operation::SetSectionPageNumbering { section, .. } => {
            Some((Target::Section(*section), Aspects::SECTION_PAGE_NUMBERING))
        }
        Operation::SetSectionVerticalAlignment { section, .. } => Some((
            Target::Section(*section),
            Aspects::SECTION_VERTICAL_ALIGNMENT,
        )),
        Operation::SetEvenAndOddHeaders { .. } => {
            Some((Target::Settings, Aspects::EVEN_AND_ODD_HEADERS))
        }
        Operation::InsertText { .. }
        | Operation::DeleteText { .. }
        | Operation::SplitParagraph { .. }
        | Operation::JoinParagraphs { .. }
        | Operation::FormatText { .. }
        | Operation::ClearFormatting { .. }
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
        | Operation::DeleteObject { .. }
        | Operation::InsertObjectNode { .. }
        | Operation::InsertInlineObject { .. }
        | Operation::RemoveInlineObject { .. }
        | Operation::ReplaceTable { .. }
        | Operation::UpdateReviewState { .. }
        | Operation::SpliceSectionBoundary { .. }
        | Operation::CreateBookmark { .. }
        | Operation::DeleteBookmark { .. }
        | Operation::InsertField { .. }
        | Operation::RemoveField { .. }
        | Operation::InsertFieldRange { .. }
        | Operation::RemoveFieldRange { .. }
        | Operation::InsertNote { .. }
        | Operation::RemoveNote { .. }
        | Operation::CreateHeaderFooterBody { .. }
        | Operation::RemoveHeaderFooterBody { .. } => None,
    }
}

/// The run-property write an operation performs over a range, when it performs one.
///
/// `None` for the delta means "every field" — a [`Operation::ClearFormatting`] removes all
/// direct character formatting, so it contends with every other field.
fn run_property_write(operation: &Operation) -> Option<(EditRange, Option<&FormatDelta>)> {
    match operation {
        Operation::FormatText { range, delta } => Some((*range, Some(delta))),
        Operation::ClearFormatting { range } => Some((*range, None)),
        _ => None,
    }
}

/// `delta` with every field the concurrent write also sets removed.
///
/// Destructured field by field with no `..`, so a twelfth formatting field is a compile
/// error here rather than a field that silently survives a concurrent write.
fn delta_without(delta: &FormatDelta, overwritten_by: Option<&FormatDelta>) -> FormatDelta {
    let FormatDelta {
        bold,
        italic,
        underline,
        underline_color,
        underline_style,
        strike,
        color,
        highlight,
        size_half_points,
        vertical_alignment,
        font,
    } = delta.clone();
    let Some(other) = overwritten_by else {
        // A clear writes every field, so nothing of the earlier delta survives it.
        return FormatDelta::default();
    };
    FormatDelta {
        bold: bold.filter(|_| other.bold.is_none()),
        italic: italic.filter(|_| other.italic.is_none()),
        underline: underline.filter(|_| other.underline.is_none()),
        underline_color: underline_color.filter(|_| other.underline_color.is_none()),
        underline_style: underline_style.filter(|_| other.underline_style.is_none()),
        strike: strike.filter(|_| other.strike.is_none()),
        color: color.filter(|_| other.color.is_none()),
        highlight: highlight.filter(|_| other.highlight.is_none()),
        size_half_points: size_half_points.filter(|_| other.size_half_points.is_none()),
        vertical_alignment: vertical_alignment.filter(|_| other.vertical_alignment.is_none()),
        font: font.filter(|_| other.font.is_none()),
    }
}

fn delta_is_empty(delta: &FormatDelta) -> bool {
    *delta == FormatDelta::default()
}

/// Decide two run-property writes over overlapping text.
///
/// Character granularity and per field, which is what `apply` itself does: `FormatText`
/// merges a delta into the runs it covers, so two concurrent writes only contend on the
/// fields they both set. A concurrent bold and a concurrent italic over the same words
/// must both survive; a concurrent bold and a concurrent un-bold must not.
///
/// The earlier operation therefore keeps its full delta **outside** the overlap and only
/// the fields the later one leaves alone **inside** it — up to three operations, which is
/// why [`Rebase::KeepMany`] carries a sequence rather than a pair. `ClearFormatting`
/// cannot be reduced (it writes everything), so its overlapping part is dropped instead.
///
/// Returns `None` when this is not a pair of run-property writes over one paragraph.
fn contend_over_text(subject: &Operation, against: &Operation, side: Side) -> Option<Rebase> {
    if side == Side::Later {
        return None;
    }
    let (mine, my_delta) = run_property_write(subject)?;
    let (theirs, their_delta) = run_property_write(against)?;
    if mine.start.node != theirs.start.node {
        return None;
    }
    let node = mine.start.node;
    let overlap_start = max(mine.start.offset, theirs.start.offset);
    let overlap_end = min(mine.end.offset, theirs.end.offset);
    if overlap_start >= overlap_end {
        return None;
    }
    let mut pieces = Vec::new();
    let mut push = |start: u32, end: u32, delta: Option<FormatDelta>| {
        if start >= end {
            return;
        }
        let range = EditRange {
            start: Pos::new(node, start),
            end: Pos::new(node, end),
        };
        pieces.push(match delta {
            Some(delta) => Operation::FormatText { range, delta },
            None => Operation::ClearFormatting { range },
        });
    };
    push(
        mine.start.offset,
        overlap_start,
        my_delta.cloned().map(Some).unwrap_or(None),
    );
    if let Some(my_delta) = my_delta {
        let reduced = delta_without(my_delta, their_delta);
        if !delta_is_empty(&reduced) {
            push(overlap_start, overlap_end, Some(reduced));
        }
    }
    push(
        overlap_end,
        mine.end.offset,
        my_delta.cloned().map(Some).unwrap_or(None),
    );
    Some(match pieces.len() {
        0 => Rebase::Satisfied,
        1 => Rebase::Keep(pieces.remove(0)),
        _ => Rebase::KeepMany(pieces),
    })
}

/// Decide two operations that write the same thing.
///
/// Node granularity, matching Word and Google Docs: two people setting one paragraph's
/// alignment is one of them winning, not a merge. The **earlier** operation yields, and
/// yields only the aspects the later one overwrites; when nothing of it is left it becomes
/// `Satisfied` rather than a tombstone, because last-writer-wins is the defined semantics
/// and not a loss this layer caused.
fn contend(subject: Operation, against: &Operation, side: Side) -> Rebase {
    if side == Side::Later {
        return Rebase::Keep(subject);
    }
    let (Some((mine, my_aspects)), Some((theirs, their_aspects))) =
        (footprint(&subject), footprint(against))
    else {
        return Rebase::Keep(subject);
    };
    if mine != theirs || !my_aspects.intersects(their_aspects) {
        return Rebase::Keep(subject);
    }
    // Every retained-value write in the op set claims its aspects atomically — there is no
    // operation that writes "the extent but not the transform" of a group, or "the title
    // page but not the watermark" of a section. So a partial yield is inexpressible, and an
    // intersecting pair yields whole. `SetGroupGeometry` meeting `SetExtent` is the one
    // asymmetric case and is the reason the mask exists at all: it collides with
    // `SetExtent` and not with `SetAnchor`.
    Rebase::Satisfied
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
