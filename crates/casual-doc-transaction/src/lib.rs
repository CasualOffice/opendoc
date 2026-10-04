// SPDX-License-Identifier: Apache-2.0

//! The transaction envelope and the ordered revision log — the one path by which a live
//! document changes (doc 147, ADR-043, ADR-005).
//!
//! # What this crate is
//!
//! Command sourcing over an append-only log, with one mutation choke point. The shape is
//! ProseMirror's (`Transaction` → steps → `StepMap`, with history derived from the
//! transaction stream) and CodeMirror 6's independently; nothing here is novel. This crate
//! owns the **envelope** — identity, revision, grouping, inverse, position map — and
//! `casual-doc-edit` owns the **operations**, which are closed by invariant I2 (doc 45) and
//! each of which already returns its own inverse.
//!
//! ```text
//!   Transaction ──apply──▶ Commit ──appended──▶ RevisionLog
//!        │                    │                     │
//!   the intent          one link in          the ordered chain;
//!                       the chain            undo/redo READ it
//! ```
//!
//! # The two rules that make this affordable on a keystroke
//!
//! `107` §4 B1 requires per-keystroke work to be O(1) in document size. So:
//!
//! - **No document clone for a single-operation transaction.** [`casual_doc_edit::apply`]
//!   validates before it mutates, and its `a_refused_operation_leaves_the_document_unchanged`
//!   asserts it, so a one-operation transaction is already all-or-nothing. A
//!   multi-operation transaction takes exactly one working copy, which is what the caller
//!   used to pay by hand.
//! - **No whole-model validation per transaction.** `24`'s Phase-0 pipeline revalidated the
//!   document on every commit. That is O(document) and stays on the v0 path (see [`v0`]),
//!   which is not a keystroke path.
//!
//! # Position space
//!
//! Positions here are **UTF-8 byte offsets** into a paragraph's plain text — the live op
//! set's anchor space, shared with hit-testing (`58` §3, `59`). [`Affinity`] is retained
//! because it is orthogonal to the unit and decides the insert-at-the-same-boundary tie OT
//! needs (`107` §3.3). The Phase-0 grapheme-addressed vocabulary is [`v0`]; ADR-043 records
//! why the live path differs and why that is deliberate.
//!
//! # Concurrency
//!
//! [`transform`] rebases one operation over a concurrent one (doc 150, ADR-045). It is a
//! pure function over a [`Change`](transform::Change) — an operation together with the
//! inverse recorded for it.
//!
//! [`protocol`], [`wire`] and [`session`] are the collaboration foundation (doc 152,
//! ADR-047): the wire vocabulary, the identity discipline that keeps two replicas from
//! minting one id twice, and the two session state machines — including the rollback/replay
//! rebase driver, which is here because this is the one place that holds the log, the
//! document and [`transform`] together.
//!
//! **OT stays dormant at one editor**, and that is guarded rather than intended. The
//! keystroke path — [`RevisionLog::apply`] and everything it calls — contains no transform
//! and no rebase, and `the_keystroke_path_runs_no_transform` fails the build if one appears.
//! [`session`] is the only module here that calls [`transform`], and nothing calls
//! [`session`] unless a host has joined a room.
//!
//! Still absent: the byte codec (the operation set has no `serde` and the op-set lane will
//! move its shapes), the node-addressed mapping steps (`107` P-4), and the relay binary,
//! presence and collaborative undo (`107` 6.6, doc 152 §9).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use casual_doc_edit::{EditError, Mint};
use casual_doc_model::NodeId;
use casual_doc_model::v1::Document;

pub mod codec;
pub mod combine;
pub mod intent;
pub mod presence;
pub mod protocol;
pub mod session;
pub mod transform;
pub mod v0;
pub mod wire;

/// The live operation vocabulary. One set, re-exported rather than re-declared: a second
/// enum over the same document is the defect doc 147 exists to remove.
pub use casual_doc_edit::Operation;
pub use intent::{AnchorError, BlockAnchor, BlockTarget, Intent, resolve_anchor};

/// Monotonic session-local document revision.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RevisionId(u64);

impl RevisionId {
    /// Creates a revision from its numeric value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next revision, or `None` when the counter is exhausted.
    #[must_use]
    fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

/// Stable identity of one transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionId(u128);

impl TransactionId {
    /// Creates a transaction ID.
    #[must_use]
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    /// Returns the numeric representation.
    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }
}

/// Identity of one undo group — the granularity a user's Undo acts on.
///
/// A coalesced typing run is many commits (the OT substrate keeps each keystroke) and one
/// group (the user pressed Undo once and expects the word back).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct GroupId(u64);

impl GroupId {
    /// Returns the numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The user-facing name of the step a commit belongs to ("Typing", "Table structure", …).
///
/// The engine owns this vocabulary so a host never has to reverse-engineer a command from
/// its inverse operations. It is `&'static str` because the log is in memory; a persisted
/// log needs a stable serialisable code (doc 147 §7 Q2).
pub type Label = &'static str;

/// Why a commit exists — the field that makes undo a *read* of the log rather than a second
/// stack beside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    /// A user edit.
    Edit,
    /// The inverse of `group`, applied as a new commit.
    Undo {
        /// The group this commit reverted.
        group: GroupId,
    },
    /// The inverse of an undo commit's group, applied as a new commit.
    Redo {
        /// The undo group this commit reverted.
        group: GroupId,
    },
}

/// How a transaction joins the undo grouping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coalesce {
    /// Open a new undo group: one user action, one Undo.
    New,
    /// Continue the newest group, contributing this commit's own inverse to it. Typing.
    Continue,
    /// Continue the newest group, contributing **no** inverse.
    ///
    /// The review-typing rule: a `SetInlines` inverse is a whole-paragraph snapshot, so a
    /// word typed in suggesting mode must not retain one snapshot per character. The group's
    /// earliest snapshot already restores the paragraph. Forward operations are still
    /// recorded in full, so nothing the OT substrate needs is lost.
    ContinueKeepingFirstInverse,
}

/// Boundary behavior when an edit occurs at a position.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Affinity {
    /// Stay before content inserted or split at the same boundary.
    Before,
    /// Move after content inserted or split at the same boundary.
    After,
}

/// A UTF-8 byte boundary inside a paragraph node, with its boundary behaviour.
///
/// Serialisable because a position is what a typed caret carries over the wire: the opaque
/// awareness payload doc 152 §2b shipped becomes a *shape* without a protocol change, and the
/// shape is this type rather than a second one. The caret type is in this crate's awareness
/// module, which this file deliberately does not name — see the module's own
/// `presence_is_never_written_to_the_revision_log`, which is why the dependency only ever
/// points inwards.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Position {
    /// Paragraph node.
    pub node: NodeId,
    /// Zero-based UTF-8 byte offset into the paragraph's plain text.
    pub offset: u32,
    /// Mapping behavior at an equal edit boundary.
    pub affinity: Affinity,
}

impl Position {
    /// A position at `offset` bytes into `node`.
    #[must_use]
    pub const fn new(node: NodeId, offset: u32, affinity: Affinity) -> Self {
        Self {
            node,
            offset,
            affinity,
        }
    }
}

/// One deterministic position-mapping step, in the live op set's byte space.
///
/// Only the four **positional** shapes emit a step. Every other operation addresses a
/// `NodeId` and moves no offset, so it emits none — which is exactly the coverage the
/// Phase-0 map had. `107` §3.2's node-addressed steps (`NodeInserted`, `NodeRemoved`,
/// `NodeReplaced`, `PropertiesChanged`) are P-4 and are not built here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingStep {
    /// Text insertion into one paragraph.
    Insert {
        /// Paragraph containing the insertion.
        node: NodeId,
        /// Byte boundary before insertion.
        at: u32,
        /// Number of inserted bytes.
        bytes: u32,
    },
    /// Text deletion inside one paragraph.
    Delete {
        /// Paragraph containing the deletion.
        node: NodeId,
        /// Inclusive deletion start, in bytes.
        start: u32,
        /// Exclusive deletion end, in bytes.
        end: u32,
    },
    /// Paragraph split.
    Split {
        /// Original paragraph.
        original: NodeId,
        /// New trailing paragraph.
        new_node: NodeId,
        /// Split boundary in the original, in bytes.
        at: u32,
    },
    /// Adjacent paragraph join.
    Join {
        /// Paragraph retaining identity.
        first: NodeId,
        /// Removed paragraph.
        second: NodeId,
        /// Former byte end of the first paragraph.
        at: u32,
    },
}

/// Ordered mapping steps produced by a committed transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PositionMap {
    steps: Vec<MappingStep>,
}

impl PositionMap {
    /// Returns mapping steps in transaction order.
    #[must_use]
    pub fn steps(&self) -> &[MappingStep] {
        &self.steps
    }

    /// Maps a position through all transaction steps.
    ///
    /// O(steps), which is O(operations in the transaction) — never O(document).
    #[must_use]
    pub fn map(&self, mut position: Position) -> Position {
        for step in &self.steps {
            match *step {
                MappingStep::Insert { node, at, bytes } if position.node == node => {
                    if position.offset > at
                        || (position.offset == at && position.affinity == Affinity::After)
                    {
                        position.offset = position.offset.saturating_add(bytes);
                    }
                }
                MappingStep::Delete { node, start, end } if position.node == node => {
                    if position.offset > start {
                        position.offset = if position.offset < end {
                            start
                        } else {
                            position.offset - (end - start)
                        };
                    }
                }
                MappingStep::Split {
                    original,
                    new_node,
                    at,
                } if position.node == original => {
                    if position.offset > at
                        || (position.offset == at && position.affinity == Affinity::After)
                    {
                        position.node = new_node;
                        position.offset -= at;
                    }
                }
                MappingStep::Join { first, second, at } if position.node == second => {
                    position.node = first;
                    position.offset = position.offset.saturating_add(at);
                }
                _ => {}
            }
        }
        position
    }
}

/// An atomic set of operations against one base revision, not yet applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    id: TransactionId,
    base_revision: RevisionId,
    label: Label,
    coalesce: Coalesce,
    origin: Origin,
    mints: Vec<Mint>,
    intents: Vec<Intent>,
    operations: Vec<Operation>,
}

impl Transaction {
    /// A user edit, opening a new undo group.
    ///
    /// `mints` is one identity space per operation, in the same order. Carrying them here
    /// rather than handing the applier a generator is what makes an application replayable:
    /// two replicas applying this transaction to one state name every node they create
    /// identically. See [`casual_doc_edit::apply`] and doc 150 §9.3.
    ///
    /// [`Transaction::reserve`] is the way to build them locally; an arrival builds them
    /// from what its sender declared on the wire.
    ///
    /// A transaction whose `mints` is shorter than its `operations` is refused at
    /// application with [`TransactionError::Edit`]`(`[`EditError::IdExhausted`]`)` rather
    /// than being quietly applied with borrowed identities.
    #[must_use]
    pub fn new(
        id: TransactionId,
        base_revision: RevisionId,
        label: Label,
        mints: Vec<Mint>,
        operations: Vec<Operation>,
    ) -> Self {
        Self {
            id,
            base_revision,
            label,
            coalesce: Coalesce::New,
            origin: Origin::Edit,
            mints,
            intents: Vec::new(),
            operations,
        }
    }

    /// Reserves an identity region wide enough for `operations`, and builds the transaction.
    ///
    /// The one place an author's generator is touched on the edit path. Everything after
    /// this point — the log, `apply`, an undo, a rebase, a remote replica — works from the
    /// region this reserved, which is why the same edit names the same nodes everywhere.
    ///
    /// `None` only when the id space is exhausted.
    #[must_use]
    pub fn reserve(
        id: TransactionId,
        base_revision: RevisionId,
        label: Label,
        ids: &mut casual_doc_model::IdGenerator,
        operations: Vec<Operation>,
    ) -> Option<Self> {
        let mints = Mint::reserve_each(ids, operations.len())?;
        Some(Self::new(id, base_revision, label, mints, operations))
    }

    /// The identity space each operation mints in, in application order.
    #[must_use]
    pub fn mints(&self) -> &[Mint] {
        &self.mints
    }

    /// The same transaction, declaring what its author knew that the operations cannot say.
    ///
    /// One [`Intent`] per operation, in the same order, exactly as `mints` is. A shorter
    /// vector is not an error: a missing entry reads as [`Intent::NONE`], which is what every
    /// caller declared before this existed, so adding the declaration is **additive** and
    /// nothing has to be updated at once. See [`crate::intent`] for why these facts travel
    /// here rather than inside the operations (doc 150 §9.1, §9.2, ADR-056).
    ///
    /// A builder rather than a parameter on [`Transaction::new`]/[`Transaction::reserve`] for
    /// the same reason: adding a parameter is a breaking change to every call site, including
    /// ones in crates this change has no business touching.
    #[must_use]
    pub fn with_intents(mut self, intents: Vec<Intent>) -> Self {
        self.intents = intents;
        self
    }

    /// What the author declared about each operation, in application order.
    ///
    /// Possibly shorter than [`Transaction::operations`]; read it through
    /// [`Transaction::intent`] rather than by index.
    #[must_use]
    pub fn intents(&self) -> &[Intent] {
        &self.intents
    }

    /// What the author declared about operation `index`, or [`Intent::NONE`].
    #[must_use]
    pub fn intent(&self, index: usize) -> Intent {
        self.intents.get(index).copied().unwrap_or(Intent::NONE)
    }

    /// The same transaction, joining the newest undo group instead of opening one.
    #[must_use]
    pub const fn coalescing(mut self, coalesce: Coalesce) -> Self {
        self.coalesce = coalesce;
        self
    }

    /// The same transaction, recorded as the revert of `group` rather than as an edit.
    ///
    /// This is what makes undo derived: the log carries *why* each commit exists, so
    /// "what does Undo target" and "is Redo available" are answered by reading the chain
    /// backwards instead of by maintaining two stacks.
    #[must_use]
    pub const fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    /// Returns the transaction ID.
    #[must_use]
    pub const fn id(&self) -> TransactionId {
        self.id
    }

    /// Returns the declared base revision.
    #[must_use]
    pub const fn base_revision(&self) -> RevisionId {
        self.base_revision
    }

    /// Returns the step label.
    #[must_use]
    pub const fn label(&self) -> Label {
        self.label
    }

    /// Returns operations in application order.
    #[must_use]
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }
}

/// One applied transaction: one link in the revision chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    id: TransactionId,
    base_revision: RevisionId,
    revision: RevisionId,
    group: GroupId,
    label: Label,
    origin: Origin,
    mints: Vec<Mint>,
    intents: Vec<Intent>,
    operations: Vec<Operation>,
    inverse_operations: Vec<Operation>,
    position_map: PositionMap,
}

impl Commit {
    /// The transaction that produced this commit.
    #[must_use]
    pub const fn id(&self) -> TransactionId {
        self.id
    }

    /// The identity space each operation minted in, in application order.
    ///
    /// A roll-back applies operation `i`'s inverse in `mints()[i].inverse()`, which is an
    /// involution, so rolling back and replaying restores the very nodes the commit first
    /// created rather than renaming them.
    #[must_use]
    pub fn mints(&self) -> &[Mint] {
        &self.mints
    }

    /// What the author declared about each operation, in application order.
    ///
    /// Retained on the commit because a *rebase* reads it: an arriving operation is rebased
    /// over the commits since its base revision, and an anchored slot stays anchored through
    /// the whole sequence. Read it through [`Commit::intent`].
    #[must_use]
    pub fn intents(&self) -> &[Intent] {
        &self.intents
    }

    /// What the author declared about operation `index`, or [`Intent::NONE`].
    #[must_use]
    pub fn intent(&self, index: usize) -> Intent {
        self.intents.get(index).copied().unwrap_or(Intent::NONE)
    }

    /// The revision this commit was applied against.
    #[must_use]
    pub const fn base_revision(&self) -> RevisionId {
        self.base_revision
    }

    /// The revision this commit produced — always `base_revision + 1`.
    #[must_use]
    pub const fn revision(&self) -> RevisionId {
        self.revision
    }

    /// The undo group this commit belongs to.
    #[must_use]
    pub const fn group(&self) -> GroupId {
        self.group
    }

    /// The user-facing step name.
    #[must_use]
    pub const fn label(&self) -> Label {
        self.label
    }

    /// Why this commit exists.
    #[must_use]
    pub const fn origin(&self) -> Origin {
        self.origin
    }

    /// The forward operations, in application order. The OT substrate.
    #[must_use]
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// The inverse operations, in the order that applying them undoes this commit.
    #[must_use]
    pub fn inverse_operations(&self) -> &[Operation] {
        &self.inverse_operations
    }

    /// Position mapping from `base_revision` to `revision`.
    #[must_use]
    pub const fn position_map(&self) -> &PositionMap {
        &self.position_map
    }

    /// This commit's changes — each forward operation paired with the inverse
    /// `casual_doc_edit::apply` returned for it — in application order.
    ///
    /// This is the shape [`transform`](crate::transform::transform) consumes, and the
    /// pairing is not obvious from the fields: `inverse_operations` is stored **reversed**,
    /// because that is the order it must be applied in to undo the commit.
    ///
    /// `None` when the commit carries no inverses. A
    /// [`Coalesce::ContinueKeepingFirstInverse`] commit records its forward operations and
    /// deliberately contributes no inverse, so it cannot say what it destroyed and cannot
    /// serve as a concurrent change (doc 150 §10 Q2).
    ///
    /// O(1) to call; O(operations) to consume.
    #[must_use]
    pub fn changes(&self) -> Option<impl ExactSizeIterator<Item = transform::Change<'_>>> {
        if self.inverse_operations.len() != self.operations.len() {
            return None;
        }
        Some(
            self.operations
                .iter()
                .zip(self.inverse_operations.iter().rev())
                .map(|(operation, inverse)| transform::Change::new(operation, inverse)),
        )
    }
}

/// The document's own history, as an ordered sequence.
///
/// Append-only: every applied transaction — forward, undo or redo — appends exactly one
/// commit and advances [`head`](Self::head) by one. Nothing pops. The only removal is
/// compaction from the FRONT, behind the undo horizon, when the group bound is reached.
///
/// This is the artifact `107` §5 replays: `Snapshot(r0) ─ op(r1) ─ … ─ op(rN)`. Durability
/// and snapshot compaction are `107` 6.1 and are not built here.
#[derive(Clone, Debug)]
pub struct RevisionLog {
    commits: VecDeque<Commit>,
    head: RevisionId,
    /// The revision up to which commits have been **ordered** by a session.
    ///
    /// Equal to `head` whenever nothing is in flight, which is every single-user session and
    /// every collaborative one between a flush and its acknowledgement. Commits above it are
    /// this replica's own unacknowledged work, and they are the only commits
    /// [`session`] ever rewrites — see [`RevisionLog::horizon`].
    horizon: RevisionId,
    /// Whether a session has ever [`settled`](RevisionLog::settle) this log.
    ///
    /// A joining client settles the log at join, so this is exactly "a session is attached".
    /// Without it the horizon cannot be read as a boundary: a single-user log never settles,
    /// so its horizon stays at revision zero while `head` climbs, and every commit would look
    /// unordered — which would switch the whole bound off for the case that has no session at
    /// all.
    settled: bool,
    next_group: u64,
    max_groups: usize,
}

/// Undo steps retained before the oldest is dropped whole.
///
/// This is a bound on *steps*, not on commits, and deliberately so: a 60-character word is
/// one step and sixty commits, so bounding commits would silently shorten undo depth for
/// anyone who types — a behaviour change disguised as a constant.
///
/// It replaces `casual-doc-wasm`'s `MAX_HISTORY_ENTRIES`, which capped an undo `Vec` and a
/// redo `Vec` at 256 each. `RevisionLog::enforce_bounds` reproduces both caps, plus a
/// ceiling on the settled groups that accumulate behind them.
pub const DEFAULT_MAX_UNDO_GROUPS: usize = 256;

/// Commits one undo group may hold before a coalescing transaction opens a new group instead.
///
/// # Why a cap on the group and not on the log
///
/// `107` §4 **B7** requires the log to be bounded, explicitly, like `21`'s `HARD_MAX_*`
/// package limits. [`DEFAULT_MAX_UNDO_GROUPS`] bounds *steps*, and deliberately so — a
/// 60-character word is one step and sixty commits — which left the **commit** count
/// unbounded: one coalescing gesture is one group and one commit per keystroke, so a long
/// dictation or a paste-driven macro grew the log without limit while the group bound looked
/// satisfied.
///
/// Capping the log directly would be worse than the disease. A group is dropped *whole* (so
/// undo never sees half a step), so a cap the current group itself exceeded would evict that
/// group — deleting the very gesture the reader is in the middle of making.
///
/// So the cap is on the group, and eviction stays exactly where it was. A run longer than this
/// **splits into a second undo step**, which is what Word and Google Docs both do with a long
/// typing run, and the existing group bound then evicts as it always has. One mechanism, not
/// two.
///
/// # The bound this yields
///
/// `group_count` is already capped at `2 × max_groups` by the log's own bound enforcement, so
/// the log holds at most `2 × max_groups × MAX_COMMITS_PER_GROUP` commits —
/// `2 × 256 × 200 = 102,400` at the defaults, which at roughly 200 bytes for a one-character
/// `InsertText` and its inverse is about 20 MB of worst case.
/// [`RevisionLog::commit_ceiling`] computes it rather than restating it, so the number in this
/// comment cannot drift away from the code.
pub const MAX_COMMITS_PER_GROUP: usize = 200;

impl Default for RevisionLog {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_UNDO_GROUPS)
    }
}

impl RevisionLog {
    /// An empty log at revision zero, retaining at most `max_groups` undo groups.
    #[must_use]
    pub fn new(max_groups: usize) -> Self {
        Self {
            commits: VecDeque::new(),
            head: RevisionId::default(),
            horizon: RevisionId::default(),
            settled: false,
            next_group: 0,
            max_groups: max_groups.max(1),
        }
    }

    /// The current document revision.
    #[must_use]
    pub const fn head(&self) -> RevisionId {
        self.head
    }

    /// The revision up to which this log's commits have been **ordered** by a session.
    ///
    /// # Why the log needs this at all
    ///
    /// Doc 147 gave the log one position, `head`, because a single-user document has one:
    /// every commit is final the moment it is applied. A collaborative one has two. A commit
    /// this replica has applied but that no relay has ordered yet is *provisional* — it may
    /// have to be re-expressed in coordinates that include somebody else's edit — and a
    /// commit below the horizon is settled and must never move.
    ///
    /// That distinction is what makes rollback/replay compatible with "nothing rewrites a
    /// commit": [`session`] rewrites only commits **above** the horizon,
    /// which are by construction this replica's own unacknowledged work, seen by nobody. A
    /// rewritten commit keeps its [`TransactionId`], [`GroupId`], [`Label`] and
    /// [`Origin`], so undo is untouched by a rebase — the user's steps are the same steps,
    /// expressed against a document that has moved.
    ///
    /// Equal to [`head`](Self::head) in every single-user session, which is why single-user
    /// editing pays nothing for any of this.
    #[must_use]
    pub const fn horizon(&self) -> RevisionId {
        self.horizon
    }

    /// Marks every commit up to and including `revision` as ordered.
    ///
    /// Called when a relay acknowledges a chunk. Never moves backwards, and never past
    /// `head` — a horizon ahead of the log would make the log's own commits unreachable to
    /// a rebase.
    pub(crate) fn settle(&mut self, revision: RevisionId) {
        self.horizon = revision.min(self.head).max(self.horizon);
        self.settled = true;
    }

    /// How many commits no relay has ordered yet.
    ///
    /// **Zero in every single-user session**, and the cheapest possible way for a caller to
    /// know that an arriving edit needs no rollback at all.
    #[must_use]
    pub fn unordered_commits(&self) -> usize {
        self.commits
            .iter()
            .filter(|commit| commit.revision > self.horizon)
            .count()
    }

    /// Takes the unordered commits off the back, oldest first, and rewinds `head` to the
    /// horizon.
    ///
    /// The document is **not** touched: the caller applies each returned commit's
    /// `inverse_operations` itself, because it is the caller that holds the document and the
    /// id allocator. Returned in application order so the caller rolls back in reverse.
    pub(crate) fn detach_unordered(&mut self) -> Vec<Commit> {
        let mut taken = Vec::new();
        while self
            .commits
            .back()
            .is_some_and(|commit| commit.revision > self.horizon)
        {
            taken.push(self.commits.pop_back().expect("just checked"));
        }
        taken.reverse();
        self.head = self.horizon;
        taken
    }

    /// Appends a commit built from `template`'s identity and `operations`'s effect.
    ///
    /// This is how a rebased commit keeps being the *same user step* while saying something
    /// different about the document. It does not resolve a coalesce group — the group comes
    /// from the template — so a rebase cannot silently merge two of the user's steps.
    ///
    /// On `Err` the document is unchanged and nothing is appended.
    pub(crate) fn append_rebased(
        &mut self,
        document: &mut Document,
        mints: Vec<Mint>,
        template: &Commit,
        operations: Vec<Operation>,
    ) -> Result<RevisionId, TransactionError> {
        if operations.is_empty() {
            return Err(TransactionError::EmptyTransaction);
        }
        let base_revision = self.head;
        let revision = base_revision
            .next()
            .ok_or(TransactionError::RevisionExhausted)?;
        let snapshot = (operations.len() > 1).then(|| document.clone());
        let mut map = PositionMap::default();
        let mut inverse_operations = Vec::with_capacity(operations.len());
        for (index, operation) in operations.iter().enumerate() {
            let step = *mints
                .get(index)
                .ok_or(TransactionError::Edit(EditError::IdExhausted))?;
            match casual_doc_edit::apply(document, step, operation) {
                Ok(inverse) => {
                    push_mapping_step(&mut map, operation, &inverse);
                    inverse_operations.push(inverse);
                }
                Err(error) => {
                    if let Some(snapshot) = snapshot {
                        *document = snapshot;
                    }
                    return Err(TransactionError::Edit(error));
                }
            }
        }
        inverse_operations.reverse();
        self.head = revision;
        self.commits.push_back(Commit {
            id: template.id,
            base_revision,
            revision,
            group: template.group,
            label: template.label,
            origin: template.origin,
            mints,
            intents: template.intents.clone(),
            operations,
            inverse_operations,
            position_map: map,
        });
        Ok(revision)
    }

    /// Commits in application order, oldest retained first.
    #[must_use]
    pub fn commits(&self) -> impl ExactSizeIterator<Item = &Commit> {
        self.commits.iter()
    }

    /// Applies `transaction` to `document` and appends the resulting commit.
    ///
    /// **This is the only way a live document changes.** On `Err` the document is exactly
    /// what it was on entry and nothing is appended.
    ///
    /// Cost is O(operations in the transaction) plus each operation's own cost — no document
    /// clone for a single operation, and no whole-model validation ever (`107` §4 B1). A
    /// multi-operation transaction clones once, which is the rollback the caller used to
    /// spell out by hand.
    ///
    /// The transaction is taken **by value** so its operations move into the commit rather
    /// than being copied into it. The log has to keep them — that is the OT substrate — and
    /// a pasted block sequence is a real payload, so the difference between one copy and two
    /// is not academic.
    pub fn apply(
        &mut self,
        document: &mut Document,
        transaction: Transaction,
    ) -> Result<&Commit, TransactionError> {
        if transaction.base_revision != self.head {
            return Err(TransactionError::StaleRevision {
                expected: transaction.base_revision,
                actual: self.head,
            });
        }
        if transaction.operations.is_empty() {
            return Err(TransactionError::EmptyTransaction);
        }
        let revision = self
            .head
            .next()
            .ok_or(TransactionError::RevisionExhausted)?;

        // One working copy for a group, none for a single operation: `casual_doc_edit::apply`
        // validates before it mutates, so a refused single operation has already left the
        // document alone. This is the rule the WASM choke point used to hold; it moved here
        // with the mutation it protects.
        let snapshot = (transaction.operations.len() > 1).then(|| document.clone());
        let mut map = PositionMap::default();
        let mut inverse_operations = Vec::with_capacity(transaction.operations.len());
        for (index, operation) in transaction.operations.iter().enumerate() {
            let step = *transaction
                .mints
                .get(index)
                .ok_or(TransactionError::Edit(EditError::IdExhausted))?;
            match casual_doc_edit::apply(document, step, operation) {
                Ok(inverse) => {
                    push_mapping_step(&mut map, operation, &inverse);
                    inverse_operations.push(inverse);
                }
                Err(error) => {
                    if let Some(snapshot) = snapshot {
                        *document = snapshot;
                    }
                    return Err(TransactionError::Edit(error));
                }
            }
        }
        inverse_operations.reverse();

        let group = self.group_for(transaction.coalesce);
        let commit = Commit {
            id: transaction.id,
            base_revision: transaction.base_revision,
            revision,
            group,
            label: transaction.label,
            origin: transaction.origin,
            mints: transaction.mints,
            intents: transaction.intents,
            operations: transaction.operations,
            inverse_operations: match transaction.coalesce {
                Coalesce::ContinueKeepingFirstInverse => Vec::new(),
                Coalesce::New | Coalesce::Continue => inverse_operations,
            },
            position_map: map,
        };
        self.head = revision;
        self.commits.push_back(commit);
        self.enforce_bounds();
        Ok(self.commits.back().expect("just pushed"))
    }

    /// The group a transaction joins: the newest one when coalescing, otherwise a fresh one.
    ///
    /// A coalescing transaction opens a **new** group once the newest one holds
    /// [`MAX_COMMITS_PER_GROUP`] commits. That is what bounds the log in commits as well as in
    /// steps (B7); see that constant for why the cap is on the group rather than on the log.
    /// The promotion is to `Coalesce::New`'s behaviour in every respect, including keeping the
    /// new group's first inverse — a group whose first commit recorded none could not be
    /// rolled back at all, so promoting is strictly the safer of the two.
    fn group_for(&mut self, coalesce: Coalesce) -> GroupId {
        match coalesce {
            Coalesce::Continue | Coalesce::ContinueKeepingFirstInverse => {
                if let Some(last) = self.commits.back().map(|commit| commit.group)
                    && self.commits_in_group(last) < MAX_COMMITS_PER_GROUP
                {
                    return last;
                }
                self.allocate_group()
            }
            Coalesce::New => self.allocate_group(),
        }
    }

    /// Commits the log retains for `group`.
    ///
    /// O(retained commits), which the bounds cap, so O(1) in document size. Called once per
    /// coalescing transaction.
    fn commits_in_group(&self, group: GroupId) -> usize {
        self.commits
            .iter()
            .filter(|commit| commit.group == group)
            .count()
    }

    /// The most commits this log can hold, derived from its own bounds.
    ///
    /// B7's explicit bound. Derived rather than declared: `enforce_bounds` caps the log at
    /// `2 × max_groups` groups and [`MAX_COMMITS_PER_GROUP`] caps each group, so this is the
    /// product — and it cannot drift from the two rules that produce it.
    #[must_use]
    pub const fn commit_ceiling(&self) -> usize {
        self.max_groups
            .saturating_mul(2)
            .saturating_mul(MAX_COMMITS_PER_GROUP)
    }

    fn allocate_group(&mut self) -> GroupId {
        let id = GroupId(self.next_group);
        self.next_group = self.next_group.saturating_add(1);
        id
    }

    /// Keeps the log bounded, in whole groups, from the front.
    ///
    /// Three caps, and the first two are the two `Vec`s the flat stacks used to be:
    ///
    /// 1. at most `max_groups` **undoable** steps;
    /// 2. at most `max_groups` **redoable** steps;
    /// 3. at most `2 * max_groups` groups in total, which is what stops the *settled*
    ///    groups — an edit and the undo that reverted it, both now unreachable — from
    ///    accumulating for ever behind an undo/redo ping-pong.
    ///
    /// A single total-group cap was tried first and is wrong: undoing appends a commit, so
    /// a log capped at 256 groups evicts an undoable edit off the front on every undo, and
    /// the user runs out of undo steps about half way through the history they were
    /// promised. `history_drops_the_oldest_action_at_the_bound` catches exactly that.
    ///
    /// Cost is O(groups²) in the worst case and O(groups) in practice — one append can only
    /// put the log one over any cap — and `groups` is bounded by the caps themselves, so
    /// this is O(1) in document size (`107` §4 B1).
    fn enforce_bounds(&mut self) {
        while self.undo_depth() > self.max_groups
            || self.redo_depth() > self.max_groups
            || self.group_count() > self.max_groups.saturating_mul(2)
        {
            if !self.drop_oldest_group() {
                return;
            }
        }
    }

    /// Drops the oldest group whole, then any leading undo/redo group left dangling by it —
    /// a group whose target has just been evicted cancels nothing and can only mislead the
    /// backward scans into cancelling a step the user can still reach.
    ///
    /// Returns whether anything was dropped, so the caller cannot spin on an empty log.
    fn drop_oldest_group(&mut self) -> bool {
        let Some(front) = self.commits.front() else {
            return false;
        };
        // Never evict what nobody has ordered yet. A commit above the horizon is this
        // replica's own unacknowledged work and is the exact input `session`'s rollback and
        // replay reads; dropping it would leave the driver unable to roll back to the state an
        // arrival has to be applied at, which is divergence rather than a shorter history.
        // Refusing here means the bound yields to correctness, and `enforce_bounds` stops
        // rather than spinning.
        //
        // Only once a session has settled the log: a single-user log never settles, so its
        // horizon sits at revision zero and reading it as a boundary would switch the bound
        // off entirely for the case with no session — which two existing guards caught the
        // moment this was written without the condition.
        if self.settled && front.revision > self.horizon {
            return false;
        }
        let oldest = front.group;
        self.drop_front_group(oldest);
        while let Some(front) = self.commits.front() {
            if matches!(front.origin, Origin::Edit) {
                break;
            }
            let dangling = front.group;
            self.drop_front_group(dangling);
        }
        true
    }

    fn drop_front_group(&mut self, group: GroupId) {
        while self
            .commits
            .front()
            .is_some_and(|commit| commit.group == group)
        {
            self.commits.pop_front();
        }
    }

    /// How many undo groups the log retains.
    fn group_count(&self) -> usize {
        self.groups_newest_first().count()
    }

    /// The group Undo would revert, if any.
    ///
    /// Read backwards: each `Undo` commit cancels the next `Edit`-or-`Redo` group found.
    /// Bounded by the group bound, so O(1) in document size.
    #[must_use]
    pub fn undo_target(&self) -> Option<GroupId> {
        let mut cancelled = 0_usize;
        for (group, origin) in self.groups_newest_first_with_id() {
            match origin {
                Origin::Undo { .. } => cancelled += 1,
                Origin::Edit | Origin::Redo { .. } => {
                    if cancelled == 0 {
                        return Some(group);
                    }
                    cancelled -= 1;
                }
            }
        }
        None
    }

    /// The group Redo would re-apply, if any.
    ///
    /// Read backwards: each `Redo` commit cancels the next `Undo` group, and an `Edit`
    /// commit ends the scan. That last clause is where "a fresh edit clears redo" comes
    /// from — it is a consequence of the log's order, not a `redo.clear()` anywhere.
    #[must_use]
    pub fn redo_target(&self) -> Option<GroupId> {
        let mut cancelled = 0_usize;
        for (group, origin) in self.groups_newest_first_with_id() {
            match origin {
                Origin::Redo { .. } => cancelled += 1,
                Origin::Undo { .. } => {
                    if cancelled == 0 {
                        return Some(group);
                    }
                    cancelled -= 1;
                }
                Origin::Edit => return None,
            }
        }
        None
    }

    /// How many user actions Undo can still reverse.
    ///
    /// The same backward scan as [`undo_target`](Self::undo_target), counted rather than
    /// stopped at the first hit. Bounded by the group bound, so O(1) in document size.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        let mut cancelled = 0_usize;
        let mut depth = 0_usize;
        for origin in self.groups_newest_first() {
            match origin {
                Origin::Undo { .. } => cancelled += 1,
                Origin::Edit | Origin::Redo { .. } => {
                    if cancelled == 0 {
                        depth += 1;
                    } else {
                        cancelled -= 1;
                    }
                }
            }
        }
        depth
    }

    /// How many undone actions Redo can still re-apply.
    #[must_use]
    pub fn redo_depth(&self) -> usize {
        let mut cancelled = 0_usize;
        let mut depth = 0_usize;
        for origin in self.groups_newest_first() {
            match origin {
                Origin::Redo { .. } => cancelled += 1,
                Origin::Undo { .. } => {
                    if cancelled == 0 {
                        depth += 1;
                    } else {
                        cancelled -= 1;
                    }
                }
                Origin::Edit => break,
            }
        }
        depth
    }

    /// Each group and its origin, newest group first — the one walk the four history reads
    /// share, so the "a group is a run of commits" rule has exactly one implementation.
    fn groups_newest_first_with_id(&self) -> impl Iterator<Item = (GroupId, Origin)> + '_ {
        let mut seen: Option<GroupId> = None;
        self.commits.iter().rev().filter_map(move |commit| {
            if seen == Some(commit.group) {
                return None;
            }
            seen = Some(commit.group);
            Some((commit.group, commit.origin))
        })
    }

    /// Each group's origin, newest group first.
    fn groups_newest_first(&self) -> impl Iterator<Item = Origin> + '_ {
        self.groups_newest_first_with_id().map(|(_, origin)| origin)
    }

    /// The user-facing label of `group` — its newest commit's.
    ///
    /// Newest, not oldest, because that reproduces the shipped behaviour exactly: the flat
    /// stack re-pushed a merged typing entry with a fresh kind on every keystroke.
    #[must_use]
    pub fn label_of(&self, group: GroupId) -> Option<Label> {
        self.commits
            .iter()
            .rev()
            .find(|commit| commit.group == group)
            .map(|commit| commit.label)
    }

    /// The operations that revert `group`, in the order they must be applied.
    ///
    /// Newest commit first, and within a commit the inverse order the commit already
    /// recorded — which is what makes a multi-operation action undo as one step.
    #[must_use]
    pub fn inverse_of(&self, group: GroupId) -> Vec<Operation> {
        self.commits
            .iter()
            .rev()
            .filter(|commit| commit.group == group)
            .flat_map(|commit| commit.inverse_operations.iter().cloned())
            .collect()
    }
}

/// Records the mapping step an operation implies, if it is one of the four positional
/// shapes. Every other operation addresses a node and moves no offset.
///
/// O(1). The offsets are already on the operation or on its inverse — nothing is recomputed
/// from the document, which is what keeps this off the keystroke budget.
fn push_mapping_step(map: &mut PositionMap, operation: &Operation, inverse: &Operation) {
    match (operation, inverse) {
        // The inserted length is read off the INVERSE, not off `text`: the edit crate strips
        // XML-forbidden characters at the choke point, so the bytes that landed can be fewer
        // than the bytes offered.
        (Operation::InsertText { at, .. }, Operation::DeleteText { range }) => {
            map.steps.push(MappingStep::Insert {
                node: at.node,
                at: at.offset,
                bytes: range.end.offset.saturating_sub(range.start.offset),
            });
        }
        (Operation::DeleteText { range }, _) => {
            map.steps.push(MappingStep::Delete {
                node: range.start.node,
                start: range.start.offset,
                end: range.end.offset,
            });
        }
        (Operation::SplitParagraph { at, new_id, .. }, _) => {
            map.steps.push(MappingStep::Split {
                original: at.node,
                new_node: *new_id,
                at: at.offset,
            });
        }
        // The join boundary is the former end of `first`, which the inverse split carries.
        (Operation::JoinParagraphs { first, second, .. }, Operation::SplitParagraph { at, .. }) => {
            map.steps.push(MappingStep::Join {
                first: *first,
                second: *second,
                at: at.offset,
            });
        }
        _ => {}
    }
}

/// Transaction validation or application failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionError {
    /// The transaction was based on a different revision.
    StaleRevision {
        /// Transaction's declared revision.
        expected: RevisionId,
        /// Current log head.
        actual: RevisionId,
    },
    /// The transaction had no operation.
    EmptyTransaction,
    /// The revision counter was exhausted.
    RevisionExhausted,
    /// An operation was refused. The document is unchanged.
    Edit(EditError),
}

impl TransactionError {
    /// The refusal already written as a sentence for the reader, when the underlying
    /// operation has one. Hosts pass these through verbatim.
    #[must_use]
    pub const fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Edit(error) => error.reason(),
            _ => None,
        }
    }
}

impl fmt::Display for TransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "transaction revision {} does not match current revision {}",
                expected.get(),
                actual.get()
            ),
            Self::EmptyTransaction => formatter.write_str("transaction has no operation"),
            Self::RevisionExhausted => formatter.write_str("revision counter is exhausted"),
            Self::Edit(error) => write!(formatter, "operation refused: {error:?}"),
        }
    }
}

impl Error for TransactionError {}

/// Distinct identity spaces for a test, without threading a generator through every helper.
///
/// Monotonic across the whole test binary, so two transactions in one test never share a
/// block and a node minted by one can never be named by another — the property production
/// gets from one generator per replica.
#[cfg(test)]
pub(crate) fn test_mints(count: usize) -> Vec<casual_doc_edit::Mint> {
    use casual_doc_edit::Mint;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A namespace no test document mints in.
    const NAMESPACE: u64 = 0x0FED_0000;
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let count = count.max(1);
    let first = NEXT.fetch_add(count as u64, Ordering::Relaxed);
    let mut ids = casual_doc_model::IdGenerator::new(NAMESPACE);
    ids.reserve_through(first.saturating_mul(Mint::BLOCK));
    Mint::reserve_each(&mut ids, count).expect("a test mint")
}

#[cfg(test)]
mod tests {
    use casual_doc_edit::{Pos, Range as EditRange};
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{BlockNode, Definitions, Paragraph, ParagraphProperties};

    use super::*;

    /// `Transaction::new` with a fresh identity space per operation, which is what the
    /// author's generator gives it in production.
    fn transaction(
        id: TransactionId,
        base_revision: RevisionId,
        label: Label,
        operations: Vec<Operation>,
    ) -> Transaction {
        let mints = test_mints(operations.len());
        Transaction::new(id, base_revision, label, mints, operations)
    }

    /// A document of `paragraphs` empty paragraphs, and their ids in order.
    fn document(paragraphs: usize) -> (Document, Vec<NodeId>, IdGenerator) {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("id");
        let mut blocks = Vec::new();
        let mut nodes = Vec::new();
        for _ in 0..paragraphs {
            let id = ids.next_id().expect("id");
            nodes.push(id);
            blocks.push(BlockNode::Paragraph(Paragraph {
                id,
                properties: ParagraphProperties::default().into(),
                inlines: Vec::new(),
            }));
        }
        let doc = Document::new(document_id, blocks, Definitions::default()).expect("document");
        (doc, nodes, ids)
    }

    fn typing(id: u128, base: RevisionId, node: NodeId, offset: u32, text: &str) -> Transaction {
        transaction(
            TransactionId::new(id),
            base,
            "Typing",
            vec![Operation::InsertText {
                at: Pos::new(node, offset),
                text: text.to_owned(),
            }],
        )
    }

    fn text_of(document: &Document, node: NodeId) -> String {
        casual_doc_edit::find_paragraph_any(document, node)
            .expect("paragraph")
            .inlines
            .iter()
            .map(|inline| match inline {
                casual_doc_model::v1::InlineNode::Run(run) => run.text.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn every_applied_transaction_advances_the_revision_by_one() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        assert_eq!(log.head().get(), 0);
        for step in 0..4_u64 {
            let tx = typing(
                u128::from(step),
                log.head(),
                nodes[0],
                u32::try_from(step).expect("small"),
                "x",
            );
            let revision = log.apply(&mut doc, tx).expect("applies").revision();
            assert_eq!(revision.get(), step + 1);
            assert_eq!(log.head(), revision);
        }
        assert_eq!(log.commits().len(), 4);
    }

    #[test]
    fn a_stale_base_revision_is_refused_and_changes_nothing() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "a"))
            .expect("first applies");
        let before = text_of(&doc, nodes[0]);
        let error = log
            .apply(&mut doc, typing(2, RevisionId::new(0), nodes[0], 0, "b"))
            .expect_err("stale base revision");
        assert!(matches!(error, TransactionError::StaleRevision { .. }));
        assert_eq!(text_of(&doc, nodes[0]), before);
        assert_eq!(log.head().get(), 1);
        assert_eq!(log.commits().len(), 1);
    }

    #[test]
    fn a_refused_group_rolls_the_document_back_and_appends_nothing() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "seed"))
            .expect("seed applies");
        let before = text_of(&doc, nodes[0]);
        let head = log.head();
        // The first operation lands, the second cannot: the group must leave no trace.
        let bad = transaction(
            TransactionId::new(2),
            head,
            "Edit",
            vec![
                Operation::InsertText {
                    at: Pos::new(nodes[0], 0),
                    text: "ok".to_owned(),
                },
                Operation::DeleteText {
                    range: EditRange {
                        start: Pos::new(nodes[0], 0),
                        end: Pos::new(nodes[0], 9_999),
                    },
                },
            ],
        );
        let error = log.apply(&mut doc, bad).expect_err("refused");
        assert!(matches!(error, TransactionError::Edit(_)));
        assert_eq!(
            text_of(&doc, nodes[0]),
            before,
            "a refused group left the document half-applied"
        );
        assert_eq!(log.head(), head, "a refused group advanced the revision");
        assert_eq!(log.commits().len(), 1);
    }

    #[test]
    fn undo_and_redo_are_read_from_the_log() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "one"))
            .expect("first");
        let first = log.undo_target().expect("something to undo");
        assert_eq!(log.label_of(first), Some("Typing"));
        assert_eq!(log.redo_target(), None, "nothing undone yet");

        let inverse = log.inverse_of(first);
        let undo = transaction(TransactionId::new(2), log.head(), "Typing", inverse)
            .with_origin(Origin::Undo { group: first });
        log.apply(&mut doc, undo).expect("undo applies");
        assert_eq!(text_of(&doc, nodes[0]), "");
        assert_eq!(log.undo_target(), None, "the only edit is undone");
        let undone = log.redo_target().expect("redo available");

        let redo = transaction(
            TransactionId::new(3),
            log.head(),
            "Typing",
            log.inverse_of(undone),
        )
        .with_origin(Origin::Redo { group: undone });
        log.apply(&mut doc, redo).expect("redo applies");
        assert_eq!(text_of(&doc, nodes[0]), "one");
        assert_eq!(log.redo_target(), None, "the redo is consumed");
        assert!(log.undo_target().is_some(), "the redone edit is undoable");
    }

    #[test]
    fn a_fresh_edit_clears_redo_without_clearing_anything() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "a"))
            .expect("edit");
        let group = log.undo_target().expect("undoable");
        let undo = transaction(
            TransactionId::new(2),
            log.head(),
            "Typing",
            log.inverse_of(group),
        )
        .with_origin(Origin::Undo { group });
        log.apply(&mut doc, undo).expect("undo");
        assert!(log.redo_target().is_some());

        log.apply(&mut doc, typing(3, log.head(), nodes[0], 0, "b"))
            .expect("fresh edit");
        assert_eq!(
            log.redo_target(),
            None,
            "a fresh edit must end the redo scan — and the log still holds every commit"
        );
        assert_eq!(log.commits().len(), 3, "nothing was popped to achieve that");
    }

    #[test]
    fn a_coalesced_run_is_many_commits_and_one_undo_step() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        for (index, ch) in "word".chars().enumerate() {
            let offset = u32::try_from(index).expect("small");
            let tx = typing(
                u128::try_from(index).expect("small"),
                log.head(),
                nodes[0],
                offset,
                &ch.to_string(),
            );
            let tx = if index == 0 {
                tx
            } else {
                tx.coalescing(Coalesce::Continue)
            };
            log.apply(&mut doc, tx).expect("keystroke");
        }
        assert_eq!(text_of(&doc, nodes[0]), "word");
        assert_eq!(
            log.commits().len(),
            4,
            "the OT substrate keeps every keystroke"
        );
        let group = log.undo_target().expect("undoable");
        assert_eq!(
            log.commits().filter(|c| c.group() == group).count(),
            4,
            "one user action is one group"
        );
        let undo = transaction(
            TransactionId::new(99),
            log.head(),
            "Typing",
            log.inverse_of(group),
        )
        .with_origin(Origin::Undo { group });
        log.apply(&mut doc, undo).expect("undo");
        assert_eq!(text_of(&doc, nodes[0]), "", "one Undo takes the whole word");
    }

    #[test]
    fn keeping_the_first_inverse_records_the_forward_ops_and_no_more_inverses() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "a"))
            .expect("first");
        log.apply(
            &mut doc,
            typing(2, log.head(), nodes[0], 1, "b")
                .coalescing(Coalesce::ContinueKeepingFirstInverse),
        )
        .expect("second");
        let group = log.undo_target().expect("undoable");
        assert_eq!(
            log.inverse_of(group).len(),
            1,
            "only the group's first inverse is retained"
        );
        assert_eq!(
            log.commits()
                .filter(|c| c.group() == group)
                .map(|c| c.operations().len())
                .sum::<usize>(),
            2,
            "both forward operations are still in the log for OT"
        );
    }

    #[test]
    fn a_long_coalescing_run_is_bounded_in_commits_and_not_only_in_undo_steps() {
        // `107` §4 B7: the log is bounded, explicitly. The group bound never was a bound on
        // commits — one coalescing gesture is one group and one commit per keystroke — so a
        // run long enough grew the log without limit while the group cap looked satisfied.
        //
        // The condition is created rather than inherited: every transaction below asks to
        // COALESCE, so under the old rule all of them would land in one group and the log
        // would hold every one of them.
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::new(2);
        let ceiling = log.commit_ceiling();
        assert_eq!(
            ceiling,
            2 * 2 * MAX_COMMITS_PER_GROUP,
            "the ceiling must be derived from the two rules that produce it"
        );

        let keystrokes = MAX_COMMITS_PER_GROUP * 3;
        for step in 0..keystrokes {
            let tx = typing(step as u128 + 1, log.head(), nodes[0], step as u32, "x");
            log.apply(&mut doc, tx.coalescing(Coalesce::Continue))
                .expect("a keystroke applies");
        }

        assert!(
            log.commits().len() <= ceiling,
            "the log holds {} commits, above its own ceiling of {ceiling}",
            log.commits().len()
        );
        let groups: std::collections::BTreeSet<_> =
            log.commits().map(super::Commit::group).collect();
        assert!(
            groups.len() > 1,
            "{keystrokes} coalescing keystrokes stayed in one group, so nothing capped the \
             commits and this guard is measuring the old rule"
        );
        assert!(
            groups
                .iter()
                .all(|group| log.commits().filter(|c| c.group == *group).count()
                    <= MAX_COMMITS_PER_GROUP),
            "a group exceeded the per-group cap, so the split is not where it claims to be"
        );
        // And the reader can still undo: splitting a run into steps must not cost the ability
        // to revert one.
        assert!(
            log.undo_target().is_some(),
            "the split left nothing undoable, which is a worse outcome than an unbounded log"
        );
    }

    #[test]
    fn the_bound_never_evicts_a_commit_no_relay_has_ordered() {
        // The correctness side of the same rule. `session`'s rollback reads exactly the
        // commits above the horizon; evicting one to satisfy a memory cap would leave the
        // driver unable to reach the state an arrival has to be applied at. So the bound
        // yields, and a log that cannot shrink stays large rather than becoming wrong.
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::new(1);
        // Settle an empty log, which is what a client does at join: from here the horizon is
        // a boundary rather than a coincidence.
        log.settle(log.head());
        for step in 0..6_u32 {
            let tx = typing(u128::from(step) + 1, log.head(), nodes[0], step, "x");
            log.apply(&mut doc, tx).expect("a keystroke applies");
        }
        assert_eq!(
            log.unordered_commits(),
            6,
            "nothing was acknowledged, so every commit is this replica's own in-flight work"
        );
        assert_eq!(
            log.commits().len(),
            6,
            "the group bound evicted work no relay has ordered, which is what makes a rebase \
             unable to roll back"
        );
        // Once they are ordered, the bound applies again — the yield is to the horizon, not a
        // licence to grow for ever.
        log.settle(log.head());
        let tx = typing(7, log.head(), nodes[0], 6, "x");
        log.apply(&mut doc, tx).expect("a keystroke applies");
        assert!(
            log.commits().len() < 7,
            "the bound did not resume once the work was ordered: {} commits retained",
            log.commits().len()
        );
    }

    #[test]
    fn the_bound_counts_undo_steps_not_commits() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::new(2);
        // Three separate actions, the middle one a coalesced run of five keystrokes.
        let mut id = 0_u128;
        let mut offset = 0_u32;
        let mut push = |log: &mut RevisionLog, doc: &mut Document, c| {
            id += 1;
            let tx = typing(id, log.head(), nodes[0], offset, "x");
            offset += 1;
            log.apply(doc, tx.coalescing(c)).expect("applies");
        };
        push(&mut log, &mut doc, Coalesce::New);
        push(&mut log, &mut doc, Coalesce::New);
        for _ in 0..4 {
            push(&mut log, &mut doc, Coalesce::Continue);
        }
        assert_eq!(log.commits().len(), 6, "no group was truncated mid-run");
        push(&mut log, &mut doc, Coalesce::New);
        let groups: std::collections::BTreeSet<_> =
            log.commits().map(super::Commit::group).collect();
        assert_eq!(groups.len(), 2, "the oldest group is dropped whole");
        assert_eq!(
            log.commits().len(),
            6,
            "a five-commit run counts as ONE step against the bound"
        );
    }

    /// Every retained step must actually undo.
    ///
    /// A single total-group cap passes the depth assertion above and still fails here,
    /// because undoing APPENDS a group: at the cap, each undo evicts an undoable edit off
    /// the front, and the user runs out of history about half way through what they were
    /// promised. The bound therefore caps undoable steps, redoable steps and total groups
    /// separately (see `enforce_bounds`).
    #[test]
    fn every_retained_step_still_undoes_at_the_bound() {
        let (mut doc, nodes, _ids) = document(1);
        let bound = 3;
        let mut log = RevisionLog::new(bound);
        for index in 0..=bound {
            let id = u128::try_from(index).expect("small");
            let offset = u32::try_from(index).expect("small");
            log.apply(&mut doc, typing(id, log.head(), nodes[0], offset, "x"))
                .expect("edit applies");
        }
        assert_eq!(
            log.undo_depth(),
            bound,
            "one action past the bound leaves exactly the bound undoable"
        );
        for step in 0..bound {
            let group = log
                .undo_target()
                .unwrap_or_else(|| panic!("step {step} of {bound} must still be undoable"));
            let undo = transaction(
                TransactionId::new(1_000 + u128::try_from(step).expect("small")),
                log.head(),
                "Typing",
                log.inverse_of(group),
            )
            .with_origin(Origin::Undo { group });
            log.apply(&mut doc, undo).expect("undo applies");
        }
        assert_eq!(log.undo_depth(), 0, "every retained step was undone");
        // One character survives: the action the bound evicted. That is the bound
        // working — the oldest step is gone, not merely unreachable — and it is
        // what a user of the flat stacks saw too.
        assert_eq!(
            text_of(&doc, nodes[0]),
            "x",
            "only the evicted action's text may remain"
        );
    }

    #[test]
    fn the_position_map_moves_a_caret_through_an_insertion() {
        let (mut doc, nodes, _ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "hello"))
            .expect("seed");
        let head = log.head();
        let commit = log
            .apply(&mut doc, typing(2, head, nodes[0], 0, "ab"))
            .expect("insert");
        let map = commit.position_map().clone();
        assert_eq!(
            map.map(Position::new(nodes[0], 3, Affinity::After)).offset,
            5,
            "a caret after the insertion shifts by the inserted bytes"
        );
        assert_eq!(
            map.map(Position::new(nodes[0], 0, Affinity::Before)).offset,
            0,
            "a caret at the boundary with Before affinity stays put"
        );
        assert_eq!(
            map.map(Position::new(nodes[0], 0, Affinity::After)).offset,
            2,
            "…and with After affinity moves past it"
        );
    }
}
