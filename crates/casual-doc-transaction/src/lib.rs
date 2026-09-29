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
//! # What is deliberately absent
//!
//! `transform`. This crate makes transform *possible* — an ordered log of invertible
//! operations with a position map per commit — and does not implement it. Transform, tier
//! classification and the node-addressed mapping steps are `107` §3 and §6.3.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use casual_doc_edit::{EditError, RunIds};
use casual_doc_model::NodeId;
use casual_doc_model::v1::Document;

pub mod v0;

/// The live operation vocabulary. One set, re-exported rather than re-declared: a second
/// enum over the same document is the defect doc 147 exists to remove.
pub use casual_doc_edit::Operation;

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Affinity {
    /// Stay before content inserted or split at the same boundary.
    Before,
    /// Move after content inserted or split at the same boundary.
    After,
}

/// A UTF-8 byte boundary inside a paragraph node, with its boundary behaviour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    operations: Vec<Operation>,
}

impl Transaction {
    /// A user edit, opening a new undo group.
    #[must_use]
    pub fn new(
        id: TransactionId,
        base_revision: RevisionId,
        label: Label,
        operations: Vec<Operation>,
    ) -> Self {
        Self {
            id,
            base_revision,
            label,
            coalesce: Coalesce::New,
            origin: Origin::Edit,
            operations,
        }
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
            next_group: 0,
            max_groups: max_groups.max(1),
        }
    }

    /// The current document revision.
    #[must_use]
    pub const fn head(&self) -> RevisionId {
        self.head
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
        ids: &mut dyn RunIds,
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
        for operation in &transaction.operations {
            match casual_doc_edit::apply(document, ids, operation) {
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
    fn group_for(&mut self, coalesce: Coalesce) -> GroupId {
        match coalesce {
            Coalesce::Continue | Coalesce::ContinueKeepingFirstInverse => {
                if let Some(last) = self.commits.back() {
                    return last.group;
                }
                self.allocate_group()
            }
            Coalesce::New => self.allocate_group(),
        }
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
        let Some(oldest) = self.commits.front().map(|commit| commit.group) else {
            return false;
        };
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

#[cfg(test)]
mod tests {
    use casual_doc_edit::{Pos, Range as EditRange};
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{BlockNode, Definitions, Paragraph, ParagraphProperties};

    use super::*;

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
        Transaction::new(
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
        let (mut doc, nodes, mut ids) = document(1);
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
            let revision = log
                .apply(&mut doc, &mut ids, tx)
                .expect("applies")
                .revision();
            assert_eq!(revision.get(), step + 1);
            assert_eq!(log.head(), revision);
        }
        assert_eq!(log.commits().len(), 4);
    }

    #[test]
    fn a_stale_base_revision_is_refused_and_changes_nothing() {
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, &mut ids, typing(1, log.head(), nodes[0], 0, "a"))
            .expect("first applies");
        let before = text_of(&doc, nodes[0]);
        let error = log
            .apply(
                &mut doc,
                &mut ids,
                typing(2, RevisionId::new(0), nodes[0], 0, "b"),
            )
            .expect_err("stale base revision");
        assert!(matches!(error, TransactionError::StaleRevision { .. }));
        assert_eq!(text_of(&doc, nodes[0]), before);
        assert_eq!(log.head().get(), 1);
        assert_eq!(log.commits().len(), 1);
    }

    #[test]
    fn a_refused_group_rolls_the_document_back_and_appends_nothing() {
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(
            &mut doc,
            &mut ids,
            typing(1, log.head(), nodes[0], 0, "seed"),
        )
        .expect("seed applies");
        let before = text_of(&doc, nodes[0]);
        let head = log.head();
        // The first operation lands, the second cannot: the group must leave no trace.
        let bad = Transaction::new(
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
        let error = log.apply(&mut doc, &mut ids, bad).expect_err("refused");
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
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(
            &mut doc,
            &mut ids,
            typing(1, log.head(), nodes[0], 0, "one"),
        )
        .expect("first");
        let first = log.undo_target().expect("something to undo");
        assert_eq!(log.label_of(first), Some("Typing"));
        assert_eq!(log.redo_target(), None, "nothing undone yet");

        let inverse = log.inverse_of(first);
        let undo = Transaction::new(TransactionId::new(2), log.head(), "Typing", inverse)
            .with_origin(Origin::Undo { group: first });
        log.apply(&mut doc, &mut ids, undo).expect("undo applies");
        assert_eq!(text_of(&doc, nodes[0]), "");
        assert_eq!(log.undo_target(), None, "the only edit is undone");
        let undone = log.redo_target().expect("redo available");

        let redo = Transaction::new(
            TransactionId::new(3),
            log.head(),
            "Typing",
            log.inverse_of(undone),
        )
        .with_origin(Origin::Redo { group: undone });
        log.apply(&mut doc, &mut ids, redo).expect("redo applies");
        assert_eq!(text_of(&doc, nodes[0]), "one");
        assert_eq!(log.redo_target(), None, "the redo is consumed");
        assert!(log.undo_target().is_some(), "the redone edit is undoable");
    }

    #[test]
    fn a_fresh_edit_clears_redo_without_clearing_anything() {
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, &mut ids, typing(1, log.head(), nodes[0], 0, "a"))
            .expect("edit");
        let group = log.undo_target().expect("undoable");
        let undo = Transaction::new(
            TransactionId::new(2),
            log.head(),
            "Typing",
            log.inverse_of(group),
        )
        .with_origin(Origin::Undo { group });
        log.apply(&mut doc, &mut ids, undo).expect("undo");
        assert!(log.redo_target().is_some());

        log.apply(&mut doc, &mut ids, typing(3, log.head(), nodes[0], 0, "b"))
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
        let (mut doc, nodes, mut ids) = document(1);
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
            log.apply(&mut doc, &mut ids, tx).expect("keystroke");
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
        let undo = Transaction::new(
            TransactionId::new(99),
            log.head(),
            "Typing",
            log.inverse_of(group),
        )
        .with_origin(Origin::Undo { group });
        log.apply(&mut doc, &mut ids, undo).expect("undo");
        assert_eq!(text_of(&doc, nodes[0]), "", "one Undo takes the whole word");
    }

    #[test]
    fn keeping_the_first_inverse_records_the_forward_ops_and_no_more_inverses() {
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, &mut ids, typing(1, log.head(), nodes[0], 0, "a"))
            .expect("first");
        log.apply(
            &mut doc,
            &mut ids,
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
    fn the_bound_counts_undo_steps_not_commits() {
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::new(2);
        // Three separate actions, the middle one a coalesced run of five keystrokes.
        let mut id = 0_u128;
        let mut offset = 0_u32;
        let mut push = |log: &mut RevisionLog, doc: &mut Document, ids: &mut IdGenerator, c| {
            id += 1;
            let tx = typing(id, log.head(), nodes[0], offset, "x");
            offset += 1;
            log.apply(doc, ids, tx.coalescing(c)).expect("applies");
        };
        push(&mut log, &mut doc, &mut ids, Coalesce::New);
        push(&mut log, &mut doc, &mut ids, Coalesce::New);
        for _ in 0..4 {
            push(&mut log, &mut doc, &mut ids, Coalesce::Continue);
        }
        assert_eq!(log.commits().len(), 6, "no group was truncated mid-run");
        push(&mut log, &mut doc, &mut ids, Coalesce::New);
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
        let (mut doc, nodes, mut ids) = document(1);
        let bound = 3;
        let mut log = RevisionLog::new(bound);
        for index in 0..=bound {
            let id = u128::try_from(index).expect("small");
            let offset = u32::try_from(index).expect("small");
            log.apply(
                &mut doc,
                &mut ids,
                typing(id, log.head(), nodes[0], offset, "x"),
            )
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
            let undo = Transaction::new(
                TransactionId::new(1_000 + u128::try_from(step).expect("small")),
                log.head(),
                "Typing",
                log.inverse_of(group),
            )
            .with_origin(Origin::Undo { group });
            log.apply(&mut doc, &mut ids, undo).expect("undo applies");
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
        let (mut doc, nodes, mut ids) = document(1);
        let mut log = RevisionLog::default();
        log.apply(
            &mut doc,
            &mut ids,
            typing(1, log.head(), nodes[0], 0, "hello"),
        )
        .expect("seed");
        let head = log.head();
        let commit = log
            .apply(&mut doc, &mut ids, typing(2, head, nodes[0], 0, "ab"))
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
