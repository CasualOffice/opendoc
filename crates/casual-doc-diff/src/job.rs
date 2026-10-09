//! The resumable diff job: phases, budget, progress, cancellation.
//!
//! # Why a job and not a function
//!
//! Diffing two documents is unambiguously O(document), and SKILL §8 is explicit:
//! anything O(document) must not run on the main thread, must show real progress,
//! and must be cancellable. A `fn diff(a, b) -> VersionDiff` can satisfy none of
//! those three from inside wasm — once it is entered, the thread is gone until it
//! returns.
//!
//! So the engine exposes a **budgeted coroutine**: [`DiffJob::step`] does at most
//! `budget` units of work and returns, the [`Progress`] it returns is real (it
//! counts blocks actually projected), and
//! [`DiffJob::cancel`] takes effect at the next step boundary. The host decides
//! the thread. That is the same shape `webapp/src/background_measure.mjs` already
//! uses to measure the rest of a long document between frames, and it is what
//! lets this work in a Worker **or** on the main thread without the engine
//! knowing which.
//!
//! # The job holds no borrow
//!
//! `step` takes the two documents again on every call. A job that borrowed them
//! could not be stored in a `#[wasm_bindgen]` struct, and a job that *owned* them
//! would force the facade to clone a live document to compare against it.
//!
//! # Complexity
//!
//! Projection **O(b)**, alignment **O(b log b)**, definitions **O(definitions)**,
//! move detection **O(u)** in unmatched blocks (a hash join, not a pairwise
//! scan), finish **O(c log c)** in change records. Every phase is bounded, and the
//! two that are not resumable mid-phase — definitions and finish — are O(the
//! definition tables) and O(the change list), neither of which scales with body
//! length.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;

use casual_doc_model::v1::Document;

use crate::align::{Step, align};
use crate::compare::{MediaDigests, compare_definitions, differing_field_paths};
use crate::inline::diff_text;
use crate::projection::{BlockKey, List, NO_PARENT, Projection, Projector, resolve_block};
use crate::record::{
    ChangeSpec, DIFF_SCHEMA, DiffAnchor, DiffChange, DiffDiagnostics, DiffFamily, DiffKind,
    FindingCode, FindingSet, Story, VersionDiff, excerpt,
};

/// How many change records one diff may carry before the rest is folded into a
/// `Truncated` finding.
///
/// A list nobody can read is not a diff. Five thousand rows is already past what
/// any reader navigates; past it the honest answer is "these two versions are
/// substantially different", said once.
pub const MAX_CHANGES: usize = 5_000;

/// How much of a block's text a record carries.
pub const EXCERPT_BYTES: usize = 160;

/// The similarity two texts need before a positional pairing is accepted as an
/// edit of the same paragraph rather than a deletion and an insertion.
///
/// Measured as the shared prefix plus the shared suffix over the longer text.
/// 0.2 is deliberately generous: calling an edit an edit is more useful than
/// splitting it, and the failure mode of being too generous (a rewritten
/// paragraph shown as a big edit) is far milder than the failure mode of being
/// too strict (every edited paragraph shown as a delete and an add, which is the
/// text-diff experience this is meant to beat).
pub const PAIR_SIMILARITY: f32 = 0.2;

/// What a step reports back.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    /// Still working. `done`/`total` are projected blocks; `total` is 0 until the
    /// projection has found it.
    Working {
        /// Blocks projected so far.
        done: u64,
        /// Blocks to project, or 0 while unknown.
        total: u64,
    },
    /// Finished; [`DiffJob::take`] has the sidecar.
    Complete,
    /// Cancelled by the host. Nothing is produced.
    Cancelled,
}

/// The phase a job is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    ProjectLeft,
    ProjectRight,
    PairStories,
    AlignBlocks,
    Definitions,
    Moves,
    Finish,
    Done,
    Cancelled,
}

/// The two sides of a comparison, plus the media digests when the host can
/// supply them.
#[derive(Clone, Copy, Debug)]
pub struct DiffSides<'a> {
    /// The older state.
    pub left: &'a Document,
    /// The newer state.
    pub right: &'a Document,
    /// Digests of the older state's media parts, keyed by part name.
    pub left_digests: Option<&'a MediaDigests>,
    /// Digests of the newer state's media parts.
    pub right_digests: Option<&'a MediaDigests>,
}

/// A sibling-list pair still to align.
#[derive(Clone, Copy, Debug)]
struct ListPair {
    left: (u32, u32),
    right: (u32, u32),
    /// Which right-side list this is, so a block removed from it can say where
    /// it stood.
    right_list: RightList,
}

/// One sibling list on the right (newer) side: its story slot and its parent
/// block, or [`NO_PARENT`] for a story's root list.
#[derive(Clone, Copy, Debug)]
struct RightList {
    story: u32,
    parent: u32,
}

/// Where an unmatched left block stood in the right side's sibling list: the
/// list and the sibling position it would occupy. See [`DiffChange::place`].
#[derive(Clone, Copy, Debug)]
struct Place {
    list: RightList,
    index: u32,
}

/// The right-side extent of one replace region inside a list being aligned:
/// the region's first sibling position, the position just past it, and where
/// the list starts in the projection.
#[derive(Clone, Copy, Debug)]
struct Region {
    list: RightList,
    first: u32,
    start: u32,
    end: u32,
}

/// A change with the sort key that puts it in document order.
#[derive(Clone, Debug)]
struct Pending {
    story: u32,
    path: Vec<u32>,
    change: DiffChange,
}

/// The resumable structural diff.
#[derive(Debug, Default)]
pub struct DiffJob {
    phase: Option<Phase>,
    left_projector: Projector,
    right_projector: Projector,
    left: Projection,
    right: Projection,
    lists: VecDeque<ListPair>,
    pending: Vec<Pending>,
    findings: FindingSet,
    unmatched_left: Vec<u32>,
    unmatched_right: Vec<u32>,
    /// Where each unmatched left block stood on the right, by left index.
    places: BTreeMap<u32, Place>,
    /// How often each subtree hash occurs on each side, built once on first
    /// use: what tells a paragraph that MOVED from one that was edited.
    occurrences: Option<(HashMap<u128, u32>, HashMap<u128, u32>)>,
    diagnostics: DiffDiagnostics,
    result: Option<VersionDiff>,
}

impl DiffJob {
    /// A job that has not started.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: Some(Phase::ProjectLeft),
            ..Self::default()
        }
    }

    /// Asks the job to stop. Takes effect immediately; no result is produced.
    pub fn cancel(&mut self) {
        self.phase = Some(Phase::Cancelled);
        self.result = None;
    }

    /// Whether the job was cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.phase == Some(Phase::Cancelled)
    }

    /// The finished sidecar, once. Returns `None` before completion, after
    /// cancellation, or on a second call.
    pub fn take(&mut self) -> Option<VersionDiff> {
        self.result.take()
    }

    /// Does at most `budget` units of work.
    ///
    /// A unit is roughly one block projected or one alignment key compared. The
    /// caller converts its own frame budget into units the way
    /// `background_measure.mjs` does — by measuring what the last tick cost.
    pub fn step(&mut self, sides: DiffSides<'_>, budget: usize) -> Progress {
        let budget = budget.max(1);
        self.diagnostics.steps = self.diagnostics.steps.saturating_add(1);
        let mut spent = 0usize;
        while spent < budget {
            match self.phase.unwrap_or(Phase::Cancelled) {
                Phase::Cancelled => return Progress::Cancelled,
                Phase::Done => break,
                Phase::ProjectLeft => {
                    let before = self.left_projector.visited;
                    let done = self.left_projector.step(sides.left, budget - spent);
                    spent += usize::try_from(self.left_projector.visited - before).unwrap_or(0) + 1;
                    if done {
                        let (projection, values, visited) =
                            std::mem::take(&mut self.left_projector).finish();
                        self.left = projection;
                        self.diagnostics.blocks_visited += visited;
                        self.diagnostics.value_hashes += values.serialized;
                        self.phase = Some(Phase::ProjectRight);
                    }
                }
                Phase::ProjectRight => {
                    let before = self.right_projector.visited;
                    let done = self.right_projector.step(sides.right, budget - spent);
                    spent +=
                        usize::try_from(self.right_projector.visited - before).unwrap_or(0) + 1;
                    if done {
                        let (projection, values, visited) =
                            std::mem::take(&mut self.right_projector).finish();
                        self.right = projection;
                        self.diagnostics.blocks_visited += visited;
                        self.diagnostics.value_hashes += values.serialized;
                        self.phase = Some(Phase::PairStories);
                    }
                }
                Phase::PairStories => {
                    spent += self.pair_stories();
                    self.phase = Some(Phase::AlignBlocks);
                }
                Phase::AlignBlocks => match self.lists.pop_front() {
                    Some(pair) => spent += self.align_list(sides, pair),
                    None => self.phase = Some(Phase::Definitions),
                },
                Phase::Definitions => {
                    spent += self.compare_definitions(sides);
                    self.phase = Some(Phase::Moves);
                }
                Phase::Moves => {
                    spent += self.detect_moves(sides);
                    self.phase = Some(Phase::Finish);
                }
                Phase::Finish => {
                    spent += self.finish();
                    self.phase = Some(Phase::Done);
                }
            }
        }
        if self.phase == Some(Phase::Done) {
            return Progress::Complete;
        }
        Progress::Working {
            done: self.diagnostics.blocks_visited
                + self.left_projector.visited
                + self.right_projector.visited,
            total: 0,
        }
    }

    /// Runs the whole job to completion in one call.
    ///
    /// **Blocking and O(document).** For native, headless and Worker hosts, where
    /// there is no frame to drop. A host on a UI thread must drive
    /// [`DiffJob::step`] instead; this method exists so tests and non-interactive
    /// callers do not each invent a driver loop.
    #[must_use]
    pub fn run(sides: DiffSides<'_>) -> Option<VersionDiff> {
        let mut job = Self::new();
        loop {
            match job.step(sides, usize::MAX / 4) {
                Progress::Complete => return job.take(),
                Progress::Cancelled => return None,
                Progress::Working { .. } => {}
            }
        }
    }

    /// Pairs the two sides' stories and queues the root list of each pair whose
    /// content differs.
    ///
    /// **O(stories log stories)**. A story whose content hash matches is skipped
    /// entirely — the Merkle short-circuit at the top of the tree.
    fn pair_stories(&mut self) -> usize {
        let mut right_by_key: BTreeMap<_, u32> = BTreeMap::new();
        for (index, slot) in self.right.stories.iter().enumerate() {
            right_by_key.insert(slot.key.clone(), u32::try_from(index).unwrap_or(u32::MAX));
        }
        let mut paired_right = vec![false; self.right.stories.len()];
        let mut queued: Vec<ListPair> = Vec::new();
        let mut removed: Vec<(u32, Story)> = Vec::new();
        for (index, slot) in self.left.stories.iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            match right_by_key.get(&slot.key) {
                Some(right_index) => {
                    paired_right[*right_index as usize] = true;
                    let other = &self.right.stories[*right_index as usize];
                    if other.content_hash == slot.content_hash {
                        continue;
                    }
                    queued.push(ListPair {
                        left: (slot.first_root, slot.first_root + slot.root_count),
                        right: (other.first_root, other.first_root + other.root_count),
                        right_list: RightList {
                            story: *right_index,
                            parent: NO_PARENT,
                        },
                    });
                }
                None => removed.push((index, slot.story.clone())),
            }
        }
        let mut added: Vec<(u32, Story)> = Vec::new();
        for (index, slot) in self.right.stories.iter().enumerate() {
            if paired_right[index] {
                continue;
            }
            added.push((u32::try_from(index).unwrap_or(u32::MAX), slot.story.clone()));
        }
        for pair in queued {
            self.lists.push_back(pair);
        }
        for (index, story) in removed {
            let change = ChangeSpec {
                family: Some(DiffFamily::Block),
                kind: Some(DiffKind::Deletion),
                left: Some(DiffAnchor {
                    story,
                    path: Vec::new(),
                    node: None,
                    start: 0,
                    end: 0,
                }),
                fields: vec!["story".to_owned()],
                ..ChangeSpec::default()
            }
            .build();
            self.push_change(index, Vec::new(), change);
        }
        for (index, story) in added {
            let change = ChangeSpec {
                family: Some(DiffFamily::Block),
                kind: Some(DiffKind::Insertion),
                right: Some(DiffAnchor {
                    story,
                    path: Vec::new(),
                    node: None,
                    start: 0,
                    end: 0,
                }),
                fields: vec!["story".to_owned()],
                ..ChangeSpec::default()
            }
            .build();
            self.push_change(index, Vec::new(), change);
        }
        self.left.stories.len() + self.right.stories.len()
    }

    /// Aligns one sibling-list pair, characterises the matched-but-changed pairs,
    /// and queues their children.
    fn align_list(&mut self, sides: DiffSides<'_>, pair: ListPair) -> usize {
        let left_keys: Vec<u128> = (pair.left.0..pair.left.1)
            .filter_map(|index| self.left.blocks.get(index as usize))
            .map(|block| block.subtree_hash)
            .collect();
        let right_keys: Vec<u128> = (pair.right.0..pair.right.1)
            .filter_map(|index| self.right.blocks.get(index as usize))
            .map(|block| block.subtree_hash)
            .collect();
        let alignment = align(&left_keys, &right_keys, pair.left.0, pair.right.0);
        self.diagnostics.comparisons += alignment.comparisons;
        if alignment.degraded {
            self.findings
                .note(FindingCode::AmbiguousMatch, "blockRegion");
        }

        // An exact key match means an identical subtree: nothing to say, and
        // nothing to walk. Everything else falls into runs of deletes and
        // inserts, which the second pass tries to pair up.
        let mut deletes: Vec<u32> = Vec::new();
        let mut inserts: Vec<u32> = Vec::new();
        let mut cost = alignment.steps.len();
        // The right-side extent of the region being accumulated, in sibling
        // positions: it starts just past the previous exact match and ends at
        // the next one (or the end of the list).
        let mut region = Region {
            list: pair.right_list,
            first: pair.right.0,
            start: 0,
            end: 0,
        };
        for step in &alignment.steps {
            match *step {
                Step::Equal(_, right) => {
                    region.end = right.saturating_sub(pair.right.0);
                    cost += self.resolve_run(sides, &mut deletes, &mut inserts, region);
                    region.start = region.end.saturating_add(1);
                }
                Step::Delete(index) => deletes.push(index),
                Step::Insert(index) => inserts.push(index),
            }
        }
        region.end = pair.right.1.saturating_sub(pair.right.0);
        cost += self.resolve_run(sides, &mut deletes, &mut inserts, region);
        cost
    }

    /// Pairs up one replace region.
    ///
    /// The second pass aligns the region on the **kind tag alone** — the same
    /// aligner, a weaker key — so a paragraph is considered against a paragraph
    /// and a table against a table, positionally.
    ///
    /// Whether a candidate pairing is accepted turns on whether the region is
    /// **ambiguous**, not on how alike the two blocks are:
    ///
    /// * The kinds line up one-to-one with nothing left over — one paragraph
    ///   replaced by one paragraph, one cell by one cell. There is no other
    ///   reading of that, so it is an edit however different the two texts are.
    ///   A cell that went from `before` to `after` is the same cell. **Except a
    ///   paragraph whose exact content is unique on both sides**: that one has
    ///   another reading — it moved — and `is_move_candidate` leaves it to move
    ///   detection rather than word-diffing it against its neighbour.
    /// * The region is ragged — three paragraphs became five. Now the pairing is
    ///   a guess, so it is only made where the texts are similar enough
    ///   ([`PAIR_SIMILARITY`]); the rest stay unmatched and go to move detection.
    ///
    /// A container is always accepted: a table whose one cell changed is still
    /// that table, and refusing it would report a thousand cells as deleted.
    ///
    /// Every left block left unmatched also records where it stood on the
    /// right ([`DiffChange::place`]): just after the right half of the nearest
    /// pairing before it in this region, or at the region's start when there
    /// is none. So a removed paragraph is put back *before* whatever replaced
    /// it, which is the order Word's and Google's redlines read in — the old
    /// text struck, then the new.
    ///
    /// **O(region log region)**.
    fn resolve_run(
        &mut self,
        sides: DiffSides<'_>,
        deletes: &mut Vec<u32>,
        inserts: &mut Vec<u32>,
        region: Region,
    ) -> usize {
        if deletes.is_empty() && inserts.is_empty() {
            return 0;
        }
        let cost = deletes.len() + inserts.len();
        if deletes.is_empty() {
            self.unmatched_right.append(inserts);
            return cost;
        }
        if inserts.is_empty() {
            for index in deletes.iter() {
                self.places.insert(
                    *index,
                    Place {
                        list: region.list,
                        index: region.start,
                    },
                );
            }
            self.unmatched_left.append(deletes);
            return cost;
        }
        let left_kinds: Vec<u128> = deletes
            .iter()
            .map(|index| u128::from(kind_tag(&self.left, *index)))
            .collect();
        let right_kinds: Vec<u128> = inserts
            .iter()
            .map(|index| u128::from(kind_tag(&self.right, *index)))
            .collect();
        let alignment = align(&left_kinds, &right_kinds, 0, 0);
        self.diagnostics.comparisons += alignment.comparisons;
        let unambiguous = alignment
            .steps
            .iter()
            .all(|step| matches!(step, Step::Equal(_, _)));
        let mut matched_left = vec![false; deletes.len()];
        let mut matched_right = vec![false; inserts.len()];
        // For each left slot, the right block it was paired with, if any.
        let mut partner: Vec<Option<u32>> = vec![None; deletes.len()];
        for step in &alignment.steps {
            if let Step::Equal(left_slot, right_slot) = *step {
                let left_index = deletes[left_slot as usize];
                let right_index = inserts[right_slot as usize];
                if !self.is_move_candidate(left_index, right_index)
                    && (unambiguous || self.accept_pair(sides, left_index, right_index))
                {
                    matched_left[left_slot as usize] = true;
                    matched_right[right_slot as usize] = true;
                    partner[left_slot as usize] = Some(right_index);
                    self.characterise(sides, left_index, right_index);
                }
            }
        }
        // One forward pass: the position just past the latest pairing seen so
        // far is where the next unmatched left block stood.
        let mut stood_at = region.start;
        for (slot, index) in deletes.iter().enumerate() {
            if let Some(right_index) = partner[slot] {
                stood_at = right_index.saturating_sub(region.first).saturating_add(1);
            } else {
                self.places.insert(
                    *index,
                    Place {
                        list: region.list,
                        index: stood_at,
                    },
                );
                self.unmatched_left.push(*index);
            }
        }
        for (slot, index) in inserts.iter().enumerate() {
            if !matched_right[slot] {
                self.unmatched_right.push(*index);
            }
        }
        deletes.clear();
        inserts.clear();
        cost
    }

    /// Whether a positional pairing of two paragraphs is really a MOVE beside an
    /// unrelated change, and so must not be called an edit.
    ///
    /// A one-to-one region is otherwise always an edit ("there is no other
    /// reading"), and that is wrong in exactly one case: either paragraph's
    /// exact content occurs **once on each side** — the uniqueness rule
    /// `detect_moves` already applies. Then there IS another reading, and it is
    /// the right one: `[We moved…, Intro, Removed line]` → `[Intro, We moved…]`
    /// used to pair "Removed line" with "We moved…" and word-diff them into
    /// "Removed~~We~~ line~~moved this sentence~~", where a reader sees one move
    /// and one removal. Left unpaired, both go to move detection and come back
    /// as exactly that.
    ///
    /// Paragraphs only: a container with one changed cell is still that
    /// container. **O(1)** after a one-off O(b) count of both sides.
    fn is_move_candidate(&mut self, left_index: u32, right_index: u32) -> bool {
        let (Some(left), Some(right)) = (
            self.left.blocks.get(left_index as usize).copied(),
            self.right.blocks.get(right_index as usize).copied(),
        ) else {
            return false;
        };
        if left.kind != BlockKey::Paragraph || right.kind != BlockKey::Paragraph {
            return false;
        }
        let (left_counts, right_counts) = self.occurrences.get_or_insert_with(|| {
            let count = |projection: &Projection| {
                let mut counts: HashMap<u128, u32> = HashMap::new();
                for block in &projection.blocks {
                    *counts.entry(block.subtree_hash).or_insert(0) += 1;
                }
                counts
            };
            (count(&self.left), count(&self.right))
        });
        let unique_on_both =
            |hash: u128| left_counts.get(&hash) == Some(&1) && right_counts.get(&hash) == Some(&1);
        unique_on_both(left.subtree_hash) || unique_on_both(right.subtree_hash)
    }

    /// Whether a candidate pairing is the same block, modified.
    fn accept_pair(&self, sides: DiffSides<'_>, left_index: u32, right_index: u32) -> bool {
        let Some(left_block) = self.left.blocks.get(left_index as usize) else {
            return false;
        };
        if left_block.kind != BlockKey::Paragraph {
            return true;
        }
        let left_text = self.text_of(sides.left, &self.left, left_index);
        let right_text = self.text_of(sides.right, &self.right, right_index);
        if left_text.is_empty() && right_text.is_empty() {
            return true;
        }
        similarity(&left_text, &right_text) >= PAIR_SIMILARITY
    }

    /// Says what changed between two blocks that are the same block.
    fn characterise(&mut self, sides: DiffSides<'_>, left_index: u32, right_index: u32) {
        let Some(left_block) = self.left.blocks.get(left_index as usize).copied() else {
            return;
        };
        let Some(right_block) = self.right.blocks.get(right_index as usize).copied() else {
            return;
        };
        if left_block.text_hash != right_block.text_hash {
            let left_text = self.text_of(sides.left, &self.left, left_index);
            let right_text = self.text_of(sides.right, &self.right, right_index);
            let diff = diff_text(&left_text, &right_text);
            self.diagnostics.comparisons += diff.comparisons;
            if diff.truncated {
                self.findings
                    .note(FindingCode::Truncated, "inlineTextBudget");
            }
            for edit in &diff.edits {
                let kind = if edit.removes() && edit.adds() {
                    // A replacement is reported as review reports one: the
                    // deletion and the insertion are both real, and the panel
                    // shows them as one row because they share an anchor.
                    DiffKind::Insertion
                } else if edit.adds() {
                    DiffKind::Insertion
                } else {
                    DiffKind::Deletion
                };
                let change = ChangeSpec {
                    family: Some(DiffFamily::Text),
                    kind: Some(kind),
                    left: Some(self.anchor(&self.left, left_index, edit.left)),
                    right: Some(self.anchor(&self.right, right_index, edit.right)),
                    left_text: edit
                        .removes()
                        .then(|| excerpt(slice(&left_text, edit.left), EXCERPT_BYTES)),
                    right_text: edit
                        .adds()
                        .then(|| excerpt(slice(&right_text, edit.right), EXCERPT_BYTES)),
                    ..ChangeSpec::default()
                }
                .build();
                self.push_change(
                    right_block.story,
                    self.right.order_key(right_index).1,
                    change,
                );
            }
        }
        if left_block.format_hash != right_block.format_hash {
            let mut fields = self.formatting_fields(sides, left_index, right_index);
            if fields.is_empty() {
                if left_block.text_hash == right_block.text_hash {
                    // Same text, same property VALUES, different run structure:
                    // a run boundary moved, so the formatting now covers
                    // different text. A real change with no single field to
                    // blame, and `runBoundary` is its honest name.
                    fields.push("runBoundary".to_owned());
                } else {
                    // The format hash differs only because the text does — the
                    // run lengths folded into it moved. Reporting that as a
                    // formatting change would put a second, false row beside
                    // every text edit.
                    return self.queue_children(left_index, right_index);
                }
            }
            let family = match left_block.kind {
                BlockKey::Row | BlockKey::Cell | BlockKey::Table => DiffFamily::Table,
                _ => DiffFamily::Formatting,
            };
            let change = ChangeSpec {
                family: Some(family),
                kind: Some(DiffKind::Formatting),
                left: Some(self.anchor(&self.left, left_index, (0, 0))),
                right: Some(self.anchor(&self.right, right_index, (0, 0))),
                fields,
                ..ChangeSpec::default()
            }
            .build();
            self.push_change(
                right_block.story,
                self.right.order_key(right_index).1,
                change,
            );
        }
        if left_block.object_hash != right_block.object_hash {
            // The objects in a paragraph — drawings, fields, math, symbols — are
            // located exactly and characterised only as far as their modeled
            // fields go. Anything this engine retains verbatim (an OMML subtree,
            // an embedded part) is named as changed and declared uncharacterised,
            // rather than being described wrongly.
            self.findings.note(FindingCode::NotCompared, "inlineObject");
            let change = ChangeSpec {
                family: Some(DiffFamily::Object),
                kind: Some(DiffKind::Property),
                left: Some(self.anchor(&self.left, left_index, (0, 0))),
                right: Some(self.anchor(&self.right, right_index, (0, 0))),
                fields: vec!["inlineObject".to_owned()],
                ..ChangeSpec::default()
            }
            .build();
            self.push_change(
                right_block.story,
                self.right.order_key(right_index).1,
                change,
            );
        }
        if left_block.review_hash != right_block.review_hash {
            let change = ChangeSpec {
                family: Some(DiffFamily::Review),
                kind: Some(DiffKind::Property),
                left: Some(self.anchor(&self.left, left_index, (0, 0))),
                right: Some(self.anchor(&self.right, right_index, (0, 0))),
                fields: vec!["revision".to_owned()],
                ..ChangeSpec::default()
            }
            .build();
            self.push_change(
                right_block.story,
                self.right.order_key(right_index).1,
                change,
            );
        }
        // Whatever the pair is, its children still have to be aligned.
        self.queue_children(left_index, right_index);
    }

    /// The typed field paths behind a formatting difference, or an empty list when
    /// the two blocks' property VALUES are identical.
    ///
    /// Paragraph, row and cell properties are all named exactly, by reflection
    /// over the model type. Character formatting is named per run with its
    /// position; when the run structures differ in count, `runProperties` is the
    /// honest name, because no single field is to blame.
    fn formatting_fields(
        &mut self,
        sides: DiffSides<'_>,
        left_index: u32,
        right_index: u32,
    ) -> Vec<String> {
        let left_block = resolve_block(sides.left, &self.left, left_index);
        let right_block = resolve_block(sides.right, &self.right, right_index);
        match (left_block, right_block) {
            (
                Some(casual_doc_model::v1::BlockNode::Paragraph(left_paragraph)),
                Some(casual_doc_model::v1::BlockNode::Paragraph(right_paragraph)),
            ) => {
                let (mut fields, truncated) = differing_field_paths(
                    &*left_paragraph.properties,
                    &*right_paragraph.properties,
                );
                if truncated {
                    self.findings
                        .note(FindingCode::Truncated, "paragraphFields");
                }
                let left_runs = run_properties(&left_paragraph.inlines);
                let right_runs = run_properties(&right_paragraph.inlines);
                if left_runs.len() == right_runs.len() {
                    for (index, (left_run, right_run)) in
                        left_runs.iter().zip(right_runs.iter()).enumerate()
                    {
                        let (run_fields, _) = differing_field_paths(left_run, right_run);
                        for field in run_fields {
                            fields.push(format!("runProperties[{index}].{field}"));
                        }
                    }
                } else if left_runs != right_runs {
                    fields.push("runProperties".to_owned());
                }
                fields
            }
            _ => self.container_fields(sides, left_index, right_index),
        }
    }

    /// The typed field paths behind a row's or cell's property difference.
    ///
    /// Rows and cells are not `BlockNode`s, so they are resolved as lists and
    /// indexed. Reflected the same way as everything else, so `gridSpan` and
    /// `verticalMerge` are named rather than "properties".
    fn container_fields(
        &mut self,
        sides: DiffSides<'_>,
        left_index: u32,
        right_index: u32,
    ) -> Vec<String> {
        let left_slot = self
            .left
            .blocks
            .get(left_index as usize)
            .map_or(usize::MAX, |block| block.sibling_index as usize);
        let right_slot = self
            .right
            .blocks
            .get(right_index as usize)
            .map_or(usize::MAX, |block| block.sibling_index as usize);
        let left = crate::projection::resolve(sides.left, &self.left, left_index);
        let right = crate::projection::resolve(sides.right, &self.right, right_index);
        match (left, right) {
            (Some(List::Rows(left_rows)), Some(List::Rows(right_rows))) => {
                match (left_rows.get(left_slot), right_rows.get(right_slot)) {
                    (Some(left_row), Some(right_row)) => {
                        differing_field_paths(&left_row.properties, &right_row.properties).0
                    }
                    _ => Vec::new(),
                }
            }
            (Some(List::Cells(left_cells)), Some(List::Cells(right_cells))) => {
                match (left_cells.get(left_slot), right_cells.get(right_slot)) {
                    (Some(left_cell), Some(right_cell)) => {
                        differing_field_paths(&left_cell.properties, &right_cell.properties).0
                    }
                    _ => Vec::new(),
                }
            }
            (Some(List::Blocks(left_blocks)), Some(List::Blocks(right_blocks))) => {
                match (left_blocks.get(left_slot), right_blocks.get(right_slot)) {
                    (
                        Some(casual_doc_model::v1::BlockNode::Table(left_table)),
                        Some(casual_doc_model::v1::BlockNode::Table(right_table)),
                    ) => {
                        let (mut fields, _) =
                            differing_field_paths(&left_table.properties, &right_table.properties);
                        if left_table.grid != right_table.grid {
                            fields.push("grid".to_owned());
                        }
                        fields
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// Queues a matched pair's children, and returns the cost of having done so.
    fn queue_children(&mut self, left_index: u32, right_index: u32) {
        let left_children = self.left.children(left_index);
        let right_children = self.right.children(right_index);
        if !left_children.is_empty() || !right_children.is_empty() {
            let story = self
                .right
                .blocks
                .get(right_index as usize)
                .map_or(0, |block| block.story);
            self.lists.push_back(ListPair {
                left: (left_children.start, left_children.end),
                right: (right_children.start, right_children.end),
                right_list: RightList {
                    story,
                    parent: right_index,
                },
            });
        }
    }

    /// The right-side insertion point an unmatched left block stood at, as an
    /// anchor ([`DiffChange::place`]). **O(depth)**.
    fn place_of(&self, left_index: u32) -> Option<DiffAnchor> {
        let place = self.places.get(&left_index)?;
        let kind = self.left.blocks.get(left_index as usize)?.kind;
        let story = self
            .right
            .stories
            .get(place.list.story as usize)?
            .story
            .clone();
        let mut path = if place.list.parent == NO_PARENT {
            Vec::new()
        } else {
            self.right.path(place.list.parent)
        };
        path.push(kind.segment(place.index));
        Some(DiffAnchor {
            story,
            path,
            node: None,
            start: 0,
            end: 0,
        })
    }

    /// Compares everything outside story content. **O(definitions)**, one shot:
    /// it reads the definition tables, which do not scale with body length.
    fn compare_definitions(&mut self, sides: DiffSides<'_>) -> usize {
        let digests = match (sides.left_digests, sides.right_digests) {
            (Some(left), Some(right)) => Some((left, right)),
            _ => None,
        };
        let changes = compare_definitions(sides.left, sides.right, digests, &mut self.findings);
        let cost = changes.len() + 1;
        for change in changes {
            self.push_change(u32::MAX, Vec::new(), change);
        }
        cost
    }

    /// Turns unmatched blocks into insertions, deletions, and move pairs.
    ///
    /// Move detection is a **hash join**: unmatched blocks are bucketed by
    /// subtree hash and a pair is a move only when the hash occurs exactly once
    /// on each side. That is `docs/140` §11.2 step 6's confidence threshold made
    /// concrete, and it is O(u) rather than the O(u²) a pairwise search would
    /// cost. Content-identical-but-ambiguous regions are reported as a deletion
    /// plus an insertion with an `AmbiguousMatch` finding, never as a guess.
    fn detect_moves(&mut self, sides: DiffSides<'_>) -> usize {
        let cost = self.unmatched_left.len() + self.unmatched_right.len() + 1;
        let mut buckets: BTreeMap<u128, (Vec<u32>, Vec<u32>)> = BTreeMap::new();
        for index in &self.unmatched_left {
            if let Some(block) = self.left.blocks.get(*index as usize) {
                buckets
                    .entry(block.subtree_hash)
                    .or_default()
                    .0
                    .push(*index);
            }
        }
        for index in &self.unmatched_right {
            if let Some(block) = self.right.blocks.get(*index as usize) {
                buckets
                    .entry(block.subtree_hash)
                    .or_default()
                    .1
                    .push(*index);
            }
        }
        let mut moved_left: Vec<u32> = Vec::new();
        let mut moved_right: Vec<u32> = Vec::new();
        for (left_indices, right_indices) in buckets.values() {
            if left_indices.len() == 1 && right_indices.len() == 1 {
                let (left_index, right_index) = (left_indices[0], right_indices[0]);
                let mut from = ChangeSpec {
                    family: Some(DiffFamily::Block),
                    kind: Some(DiffKind::MoveFrom),
                    left: Some(self.anchor(&self.left, left_index, (0, 0))),
                    left_text: self.excerpt_of(sides.left, &self.left, left_index),
                    heuristic: true,
                    ..ChangeSpec::default()
                }
                .build();
                from.place = self.place_of(left_index);
                let mut to = ChangeSpec {
                    family: Some(DiffFamily::Block),
                    kind: Some(DiffKind::MoveTo),
                    right: Some(self.anchor(&self.right, right_index, (0, 0))),
                    right_text: self.excerpt_of(sides.right, &self.right, right_index),
                    heuristic: true,
                    ..ChangeSpec::default()
                }
                .build();
                to.paired_with = Some(from.id.clone());
                from.paired_with = Some(to.id.clone());
                let left_story = self.left.order_key(left_index);
                let right_story = self.right.order_key(right_index);
                self.push_change(left_story.0, left_story.1, from);
                self.push_change(right_story.0, right_story.1, to);
                moved_left.push(left_index);
                moved_right.push(right_index);
            } else if !left_indices.is_empty() && !right_indices.is_empty() {
                self.findings
                    .note(FindingCode::AmbiguousMatch, "blockMoveCandidate");
            }
        }
        let unmatched_left = std::mem::take(&mut self.unmatched_left);
        for index in unmatched_left {
            if moved_left.contains(&index) {
                continue;
            }
            let mut change = ChangeSpec {
                family: Some(family_of(&self.left, index)),
                kind: Some(DiffKind::Deletion),
                left: Some(self.anchor(&self.left, index, (0, 0))),
                left_text: self.excerpt_of(sides.left, &self.left, index),
                ..ChangeSpec::default()
            }
            .build();
            change.place = self.place_of(index);
            let key = self.left.order_key(index);
            self.push_change(key.0, key.1, change);
        }
        let unmatched_right = std::mem::take(&mut self.unmatched_right);
        for index in unmatched_right {
            if moved_right.contains(&index) {
                continue;
            }
            let change = ChangeSpec {
                family: Some(family_of(&self.right, index)),
                kind: Some(DiffKind::Insertion),
                right: Some(self.anchor(&self.right, index, (0, 0))),
                right_text: self.excerpt_of(sides.right, &self.right, index),
                ..ChangeSpec::default()
            }
            .build();
            let key = self.right.order_key(index);
            self.push_change(key.0, key.1, change);
        }
        cost
    }

    /// Sorts, counts and seals the sidecar. **O(c log c)**.
    fn finish(&mut self) -> usize {
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by(|a, b| {
            (a.story, &a.path, a.change.family, a.change.kind).cmp(&(
                b.story,
                &b.path,
                b.change.family,
                b.change.kind,
            ))
        });
        let cost = pending.len() + 1;
        if pending.len() > MAX_CHANGES {
            pending.truncate(MAX_CHANGES);
            self.findings.note(FindingCode::Truncated, "changes");
        }
        let changes: Vec<DiffChange> = pending.into_iter().map(|entry| entry.change).collect();
        let mut kind_counts: BTreeMap<DiffKind, u32> = BTreeMap::new();
        let mut family_counts: BTreeMap<DiffFamily, u32> = BTreeMap::new();
        for change in &changes {
            *kind_counts.entry(change.kind).or_insert(0) += 1;
            *family_counts.entry(change.family).or_insert(0) += 1;
        }
        let findings = std::mem::take(&mut self.findings);
        let complete = findings.is_empty();
        self.result = Some(VersionDiff {
            schema: DIFF_SCHEMA,
            left: self.left.summary,
            right: self.right.summary,
            changes,
            findings: findings.into_vec(),
            kind_counts: kind_counts.into_iter().collect(),
            family_counts: family_counts.into_iter().collect(),
            complete,
            diagnostics: self.diagnostics,
        });
        cost
    }

    /// Records a change with its document-order key.
    fn push_change(&mut self, story: u32, path: Vec<u32>, change: DiffChange) {
        if self.pending.len() >= MAX_CHANGES {
            self.findings.note(FindingCode::Truncated, "changes");
            return;
        }
        self.pending.push(Pending {
            story,
            path,
            change,
        });
    }

    /// One side's anchor for a block, with a byte range inside its text.
    fn anchor(&self, projection: &Projection, index: u32, range: (u32, u32)) -> DiffAnchor {
        let block = projection.blocks.get(index as usize);
        DiffAnchor {
            story: projection
                .stories
                .get(block.map_or(usize::MAX, |block| block.story as usize))
                .map_or(Story::Definitions, |slot| slot.story.clone()),
            path: projection.path(index),
            node: block.map(|block| block.node.to_string()),
            start: range.0,
            end: range.1,
        }
    }

    /// A block's projected text, or empty for a container. **O(the block)**.
    fn text_of(&self, document: &Document, projection: &Projection, index: u32) -> String {
        resolve_block(document, projection, index)
            .and_then(crate::projection::block_text)
            .unwrap_or_default()
    }

    /// A bounded excerpt of a block's text, when it has any.
    fn excerpt_of(
        &self,
        document: &Document,
        projection: &Projection,
        index: u32,
    ) -> Option<String> {
        let text = self.text_of(document, projection, index);
        (!text.is_empty()).then(|| excerpt(&text, EXCERPT_BYTES))
    }
}

/// A block's kind tag, for the weak second-pass alignment key.
fn kind_tag(projection: &Projection, index: u32) -> u8 {
    projection
        .blocks
        .get(index as usize)
        .map_or(0, |block| match block.kind {
            BlockKey::Paragraph => 1,
            BlockKey::Table => 2,
            BlockKey::Row => 3,
            BlockKey::Cell => 4,
            BlockKey::Sdt => 5,
            BlockKey::AltChunk => 6,
        })
}

/// The family a whole-block insertion or deletion belongs to.
fn family_of(projection: &Projection, index: u32) -> DiffFamily {
    match projection
        .blocks
        .get(index as usize)
        .map(|block| block.kind)
    {
        Some(BlockKey::Row | BlockKey::Cell) => DiffFamily::Table,
        _ => DiffFamily::Block,
    }
}

/// The run properties of a paragraph's text runs, in order. **O(inlines)**.
fn run_properties(
    inlines: &[casual_doc_model::v1::InlineNode],
) -> Vec<casual_doc_model::v1::RunProperties> {
    let mut out = Vec::new();
    collect_run_properties(inlines, &mut out);
    out
}

fn collect_run_properties(
    inlines: &[casual_doc_model::v1::InlineNode],
    out: &mut Vec<casual_doc_model::v1::RunProperties>,
) {
    use casual_doc_model::v1::InlineNode;
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push((*run.properties).clone()),
            InlineNode::Hyperlink(hyperlink) => collect_run_properties(&hyperlink.inlines, out),
            InlineNode::Revision(revision) => collect_run_properties(&revision.inlines, out),
            InlineNode::Sdt(sdt) => collect_run_properties(&sdt.inlines, out),
            InlineNode::Field(field) => collect_run_properties(&field.inlines, out),
            _ => {}
        }
    }
}

/// A byte slice of `text`, clamped to its bounds and to char boundaries.
fn slice(text: &str, range: (u32, u32)) -> &str {
    let start = (range.0 as usize).min(text.len());
    let end = (range.1 as usize).clamp(start, text.len());
    let mut start = start;
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = end;
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    &text[start..end]
}

/// How alike two texts are: shared prefix plus shared suffix over the longer
/// text. **O(min length)**, no allocation — which is why it can be asked of every
/// candidate pairing.
fn similarity(left: &str, right: &str) -> f32 {
    let longest = left.len().max(right.len());
    if longest == 0 {
        return 1.0;
    }
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut prefix = 0;
    while prefix < left_bytes.len()
        && prefix < right_bytes.len()
        && left_bytes[prefix] == right_bytes[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < left_bytes.len() - prefix
        && suffix < right_bytes.len() - prefix
        && left_bytes[left_bytes.len() - 1 - suffix] == right_bytes[right_bytes.len() - 1 - suffix]
    {
        suffix += 1;
    }
    (prefix + suffix) as f32 / longest as f32
}
