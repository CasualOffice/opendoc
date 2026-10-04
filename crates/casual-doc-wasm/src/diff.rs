//! The version-diff facade: two checkpoints in, one typed sidecar out
//! (`docs/140` §11, H3).
//!
//! # What a host calls
//!
//! ```js
//! const job = beginVersionDiff(olderBytes, newerBytes);
//! // Drive it from idle time; the budget is blocks, not milliseconds.
//! while (job.step(4000) === "working") { await nextIdle(); }
//! const diff = JSON.parse(job.result());
//! ```
//!
//! One entry point covers all three of `docs/139` §9.1's comparison modes,
//! because every one of them is two byte arrays: a version against the previous
//! version, a version against the current document (the head checkpoint the
//! store already holds), or two explicitly chosen versions.
//!
//! # What it costs, and where it runs
//!
//! A checkpoint is a **fidelity-complete source-format artifact** — real DOCX,
//! ODT, RTF or TXT bytes, not normalized JSON, because `docs/112` measured that
//! JSON omits binary resources and the retained source envelope. So a diff parses
//! two real documents, and its floor is the cost of opening two documents. It
//! does **not** lay them out: [`import_for_diff`] runs the format registry and
//! stops, so a diff skips pagination and shaping entirely, which is the larger
//! half of what opening a document costs.
//!
//! Both parses and the whole comparison run inside [`WasmVersionDiff::step`], so
//! the host owns the thread. **There is no Worker in `webapp/` today** — not one
//! `new Worker` in the whole tree — and putting the engine in one is not a small
//! change: the wasm module is instantiated once by `main.js` on the main thread
//! and holds the live document, and sharing that memory with a worker needs
//! `SharedArrayBuffer`, which needs COOP/COEP response headers that GitHub Pages
//! cannot send. A *separate* instance in a worker needs no shared memory and is
//! the right home for this — which is exactly why this facade takes bytes in and
//! hands JSON out, referencing nothing in the live session. Moving it into a
//! worker is then a `webapp/` change with no engine change at all.
//!
//! Until that happens the main thread drives it in slices, the way
//! `webapp/src/background_measure.mjs` already measures the rest of a long
//! document between frames, and `cancel()` stops it at the next slice boundary.
//!
//! # Complexity
//!
//! Parsing is O(bytes) per side and is **not** interruptible mid-parse — the
//! importer is a streaming pass with no resume point, so the worst single slice
//! costs one document parse, which is the cost the product already pays to open a
//! preview. Everything after that is O(b log b) in blocks and is interruptible at
//! every slice. See `casual_doc_diff` for the per-phase complexity.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_diff::{DiffJob, DiffSides, MediaDigests, Progress};
// Applying a comparison as tracked changes (ADR-061): the sidecar read back, and
// the one walk that resolves a change's path against a document this session
// holds. Separate `use` lines, in sorted position, per the parallel-lane import
// rule.
use casual_doc_diff::projection::{block_at_path, block_text};
use casual_doc_diff::record::{DIFF_SCHEMA, DiffChange, DiffFamily, DiffKind, Story, VersionDiff};
use casual_doc_edit::ParagraphIndex;
use casual_doc_edit::Pos;
use casual_doc_edit::refused;
use casual_doc_io::{DetectionRequest, FormatSelection, builtin_registry_with_limits};
use casual_doc_layout::flow::append_node_plain_text;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, InlineNode, ReviewProjection, Revision, RevisionGroup, RevisionGroupKind,
    RevisionKind, Run, RunProperties,
};
use wasm_bindgen::prelude::*;

use crate::{
    EditResult, HistoryKind, NoteAnchorLengths, WasmDocument, collect_paragraph_revisions_all,
    collect_review_format_ids_all, collect_review_revision_ids_all, inline_anchor_len,
    insert_review_revision, to_js, update_review_operation_across,
    validate_authored_revision_author, viewer_limits, viewer_text_limits, wrap_review_deletion,
};

/// The default slice size a host that has no measurement yet can pass.
///
/// Blocks, not milliseconds: a millisecond budget cannot be honoured by a call
/// that has already started, and the caller is the only one who knows what its
/// frame costs. `background_measure.mjs` converges on the block count that fits
/// its own budget; this is the value to start that convergence from.
pub const DEFAULT_DIFF_SLICE: u32 = 4_000;

/// [`DEFAULT_DIFF_SLICE`], for a host that would otherwise hardcode it.
///
/// A number typed into `version_panel.mjs` is a number with no committed
/// artifact behind it, which is the `105` EV-002 shape. The engine owns the
/// starting slice because the engine knows what a unit of its own work is.
#[wasm_bindgen(js_name = defaultDiffSlice)]
#[must_use]
pub fn default_diff_slice() -> u32 {
    DEFAULT_DIFF_SLICE
}

/// Which phase a job is in, as the string `step` returns.
const PHASE_PARSING: &str = "parsing";
const PHASE_WORKING: &str = "working";
const PHASE_COMPLETE: &str = "complete";
const PHASE_CANCELLED: &str = "cancelled";

/// A resumable comparison of two checkpoints.
///
/// Owns both parsed sides, so it references nothing in the live editing session
/// and can be created, driven and dropped without touching it.
#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmVersionDiff {
    left_bytes: Vec<u8>,
    right_bytes: Vec<u8>,
    left: Option<Side>,
    right: Option<Side>,
    job: DiffJob,
    result: Option<String>,
    cancelled: bool,
    total_blocks: u32,
    projected: u64,
}

/// One parsed side.
#[derive(Debug)]
struct Side {
    document: Document,
    digests: MediaDigests,
    blocks: u32,
}

/// Starts a comparison of two checkpoints. Nothing is parsed yet.
///
/// The older state is `left` and the newer is `right`; insertions are what the
/// right side has and the left does not, which is the same orientation review
/// uses.
///
/// **O(bytes copied)** — the two byte arrays cross the boundary once and are held
/// until the job's phases consume them.
#[wasm_bindgen(js_name = beginVersionDiff)]
#[must_use]
pub fn begin_version_diff(left: Vec<u8>, right: Vec<u8>) -> WasmVersionDiff {
    WasmVersionDiff {
        left_bytes: left,
        right_bytes: right,
        left: None,
        right: None,
        job: DiffJob::new(),
        result: None,
        cancelled: false,
        total_blocks: 0,
        projected: 0,
    }
}

/// Compares two checkpoints in one blocking call and returns the sidecar JSON.
///
/// **Blocking and O(document).** For a Worker, a native host, or a headless
/// caller. A host on a UI thread must use [`begin_version_diff`] and drive
/// [`WasmVersionDiff::step`]; this is here so those callers do not each write
/// their own driver loop, not as a shortcut past the budget.
///
/// # Errors
///
/// Throws when either side fails admission or import.
#[wasm_bindgen(js_name = diffVersions)]
pub fn diff_versions(left: &[u8], right: &[u8]) -> Result<String, JsValue> {
    diff_versions_inner(left, right).map_err(to_js)
}

/// [`diff_versions`] without the `JsValue`, so native tests exercise the same
/// path the boundary does.
pub(crate) fn diff_versions_inner(left: &[u8], right: &[u8]) -> Result<String, String> {
    let left = import_for_diff(left)?;
    let right = import_for_diff(right)?;
    let diff = DiffJob::run(DiffSides {
        left: &left.document,
        right: &right.document,
        left_digests: Some(&left.digests),
        right_digests: Some(&right.digests),
    })
    .ok_or_else(|| "diff cancelled".to_owned())?;
    diff.to_json()
        .map_err(|error| format!("serialize diff: {error}"))
}

#[wasm_bindgen]
impl WasmVersionDiff {
    /// Does at most `budget` blocks of work and returns the new state:
    /// `"parsing"`, `"working"`, `"complete"` or `"cancelled"`.
    ///
    /// A `"parsing"` slice costs one whole document parse and cannot be cut
    /// short; every later slice honours the budget. Pass
    /// [`DEFAULT_DIFF_SLICE`] until the host has measured what its frame affords.
    ///
    /// # Errors
    ///
    /// Throws when either checkpoint fails admission or import — a corrupt or
    /// oversized checkpoint is reported, not silently compared as if empty.
    pub fn step(&mut self, budget: u32) -> Result<String, JsValue> {
        self.step_inner(budget).map_err(to_js)
    }

    /// The sidecar JSON, once the state is `"complete"`. `undefined` before that,
    /// after a cancellation, and on a second call.
    #[must_use]
    pub fn result(&mut self) -> Option<String> {
        self.result.take()
    }

    /// Stops the job. Takes effect immediately; no result is produced.
    ///
    /// A half-finished diff is worse than none, because a reader cannot tell it
    /// is half-finished.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.job.cancel();
        self.result = None;
        // The bytes are the largest thing held, and a cancelled job will not
        // need them again.
        self.left_bytes = Vec::new();
        self.right_bytes = Vec::new();
    }

    /// Blocks projected so far. Real progress: it counts work actually done.
    #[wasm_bindgen(js_name = blocksProjected)]
    #[must_use]
    pub fn blocks_projected(&self) -> f64 {
        self.projected as f64
    }

    /// Blocks to project in total, or 0 while the second side is still being
    /// parsed. A host shows an indeterminate bar until this is non-zero and a
    /// determinate one after — rather than a bar that pretends to know.
    #[wasm_bindgen(js_name = blocksTotal)]
    #[must_use]
    pub fn blocks_total(&self) -> f64 {
        f64::from(self.total_blocks)
    }
}

impl WasmVersionDiff {
    /// [`WasmVersionDiff::step`] without the `JsValue`.
    pub(crate) fn step_inner(&mut self, budget: u32) -> Result<String, String> {
        if self.cancelled {
            return Ok(PHASE_CANCELLED.to_owned());
        }
        if self.result.is_some() {
            return Ok(PHASE_COMPLETE.to_owned());
        }
        if self.left.is_none() {
            let bytes = std::mem::take(&mut self.left_bytes);
            let side = import_for_diff(&bytes)?;
            self.left = Some(side);
            return Ok(PHASE_PARSING.to_owned());
        }
        if self.right.is_none() {
            let bytes = std::mem::take(&mut self.right_bytes);
            let side = import_for_diff(&bytes)?;
            self.total_blocks = self
                .left
                .as_ref()
                .map_or(0, |left| left.blocks)
                .saturating_add(side.blocks);
            self.right = Some(side);
            return Ok(PHASE_PARSING.to_owned());
        }
        let (Some(left), Some(right)) = (self.left.as_ref(), self.right.as_ref()) else {
            return Err("diff sides are not both parsed".to_owned());
        };
        let progress = self.job.step(
            DiffSides {
                left: &left.document,
                right: &right.document,
                left_digests: Some(&left.digests),
                right_digests: Some(&right.digests),
            },
            budget.max(1) as usize,
        );
        match progress {
            Progress::Working { done, .. } => {
                self.projected = done;
                Ok(PHASE_WORKING.to_owned())
            }
            Progress::Cancelled => {
                self.cancelled = true;
                Ok(PHASE_CANCELLED.to_owned())
            }
            Progress::Complete => {
                self.projected = u64::from(self.total_blocks);
                let diff = self
                    .job
                    .take()
                    .ok_or_else(|| "the completed diff produced no sidecar".to_owned())?;
                self.result = Some(
                    diff.to_json()
                        .map_err(|error| format!("serialize diff: {error}"))?,
                );
                // The two parsed documents are the largest thing held and the
                // sidecar does not reference them.
                self.left = None;
                self.right = None;
                Ok(PHASE_COMPLETE.to_owned())
            }
        }
    }
}

/// Imports one checkpoint for comparison: the format registry and nothing else.
///
/// **No layout.** A diff compares a document, not its pagination, so the shaper
/// and the paginator — the expensive half of opening a document — are skipped.
/// Admission limits are the viewer's, so a checkpoint this engine would refuse to
/// open is refused here too rather than becoming a special case.
///
/// **O(bytes)**, not interruptible: the importer is a streaming pass with no
/// resume point.
fn import_for_diff(bytes: &[u8]) -> Result<Side, String> {
    if let Some(refusal) = crate::viewer_admission_error(bytes) {
        return Err(refusal);
    }
    let registry = builtin_registry_with_limits(viewer_limits(), viewer_text_limits());
    let imported = registry
        .import(
            DetectionRequest {
                bytes,
                selection: FormatSelection::Auto,
                file_name_hint: None,
                mime_hint: None,
            },
            true,
        )
        .map_err(|error| format!("import version: {error}"))?;
    let digests = imported
        .resources
        .as_map()
        .iter()
        .map(|(part, bytes)| {
            let mut hasher = casual_doc_diff::hash::ContentHasher::new();
            hasher.write(bytes);
            (part.clone(), hasher.finish())
        })
        .collect();
    let blocks = count_projectable_blocks(&imported.document);
    Ok(Side {
        document: imported.document,
        digests,
        blocks,
    })
}

/// How many blocks the projection will visit, for the progress denominator.
///
/// Counts the same nodes `casual_doc_diff::projection` counts — body, headers,
/// footers, notes and comments, down through tables — so the bar reaches its end
/// rather than stopping at 40%. **O(blocks)**, no allocation.
fn count_projectable_blocks(document: &Document) -> u32 {
    let mut total = count_blocks(document.body());
    let definitions = document.definitions();
    for (_, header) in definitions.headers.iter() {
        total = total.saturating_add(count_blocks(&header.blocks));
    }
    for (_, footer) in definitions.footers.iter() {
        total = total.saturating_add(count_blocks(&footer.blocks));
    }
    for (_, note) in definitions
        .footnotes
        .iter()
        .chain(definitions.endnotes.iter())
    {
        total = total.saturating_add(count_blocks(&note.blocks));
    }
    for (_, comment) in definitions.comments.iter() {
        total = total.saturating_add(count_blocks(&comment.blocks));
    }
    total
}

/// Blocks, rows and cells in one list, recursively.
fn count_blocks(blocks: &[BlockNode]) -> u32 {
    let mut total = 0u32;
    for block in blocks {
        total = total.saturating_add(1);
        match block {
            BlockNode::Table(table) => {
                for row in &table.rows {
                    total = total.saturating_add(1);
                    for cell in &row.cells {
                        total = total
                            .saturating_add(1)
                            .saturating_add(count_blocks(&cell.blocks));
                    }
                }
            }
            BlockNode::Sdt(sdt) => total = total.saturating_add(count_blocks(&sdt.blocks)),
            BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
        }
    }
    total
}

// =============================================================================
// Applying a comparison to the open document as tracked changes (ADR-061).
// =============================================================================

/// How much removed text a change record can carry verbatim: `casual-doc-diff`'s
/// own excerpt bound, read from that crate rather than repeated here.
const VERBATIM_TEXT_BYTES: usize = casual_doc_diff::job::EXCERPT_BYTES;

/// One paragraph-local edit a comparison asks for, already translated into the
/// **review anchor** offsets the review primitives take.
#[derive(Clone, Debug)]
struct ComparisonEdit {
    /// The paragraph in THIS document, resolved from the change's right-hand
    /// path (the only coordinate that survives the comparison's re-import).
    node: NodeId,
    /// Review-anchor range of text this document has and the compared one did
    /// not. `start == end` for a pure deletion, which adds no text here.
    start: u32,
    /// End of that range.
    end: u32,
    /// What to mark `start..end` as, or `None` when nothing here is new.
    marked: Option<RevisionKind>,
    /// Text the compared document had and this one does not, to record as a
    /// struck-through deletion at `start`. **Verbatim or absent** — never an
    /// excerpt (see `verbatim_removed_text`).
    removed: Option<String>,
}

/// Records one thing a comparison asked for that tracked changes cannot say,
/// and returns `None` so the caller can `return` it.
fn unapplied(loss: &mut BTreeSet<&'static str>, key: &'static str) -> Option<ComparisonEdit> {
    loss.insert(key);
    None
}

/// The loss key for a change family that has no inline revision form at all.
///
/// `RevisionKind` is `Insertion`/`Deletion`/`MoveFrom`/`MoveTo` and nothing
/// else, so a formatting, style, section, definition, resource, comment or
/// metadata difference cannot be *expressed* as a tracked change, however
/// faithfully it was detected. Each is named rather than folded into one key,
/// because "the page setup differs" and "a style was redefined" are different
/// sentences for the reader.
const fn family_loss_key(family: DiffFamily) -> &'static str {
    match family {
        DiffFamily::Block => "block",
        DiffFamily::Text => "text",
        DiffFamily::Formatting => "formatting",
        DiffFamily::Style => "style",
        DiffFamily::Table => "table",
        DiffFamily::Object => "object",
        DiffFamily::Section => "section",
        DiffFamily::Definition => "definition",
        DiffFamily::Resource => "resource",
        DiffFamily::Comment => "comment",
        DiffFamily::Review => "review",
        DiffFamily::Metadata => "metadata",
    }
}

/// The plain-text length of one inline in the projection `casual-doc-diff`
/// records its offsets in.
///
/// Measured by **calling the projection itself** rather than by a parallel match
/// over `InlineNode`, which is the mistake `inline_anchor_len`'s own comment
/// records being made twice already: two functions that compute a length from
/// the same tree drift, and the symptom is a revision placed a few bytes short.
/// O(the inline).
fn inline_plain_len(inline: &InlineNode) -> u32 {
    let mut text = String::new();
    append_node_plain_text(
        std::slice::from_ref(inline),
        ReviewProjection::FinalWithMarkup,
        &mut text,
    );
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

/// Translates a byte offset in a paragraph's **projected plain text** — the
/// space a `DiffAnchor` records — into the **review anchor** offset the review
/// primitives and the caret use.
///
/// The two spaces are not the same, which `DiffAnchor`'s own documentation used
/// to claim: a tab contributes one byte to projected text and none to the anchor
/// space, while a note reference, an equation and a labelled embedded object
/// contribute to the anchor space and nothing to projected text. Applying a
/// diff offset directly would therefore place a revision correctly in a
/// paragraph of plain runs and silently misplace it in a paragraph with a tab —
/// which is most tabular-looking documents.
///
/// Returns `None` when the offset falls *inside* an inline whose two lengths
/// disagree, because there is no honest answer there; the caller reports the
/// change as unapplied rather than guessing. O(the paragraph's inlines).
fn plain_offset_to_anchor_offset(
    notes: &NoteAnchorLengths,
    inlines: &[InlineNode],
    target: u32,
) -> Option<u32> {
    let mut plain = 0_u32;
    let mut anchor = 0_u32;
    for inline in inlines {
        if plain == target {
            return Some(anchor);
        }
        let plain_len = inline_plain_len(inline);
        let anchor_len = inline_anchor_len(notes, inline);
        if target < plain.saturating_add(plain_len) {
            let inner = target - plain;
            return match inline {
                // A run and a symbol occupy the same bytes in both spaces, so an
                // interior offset maps straight through.
                InlineNode::Run(_) | InlineNode::Symbol(_) if plain_len == anchor_len => {
                    Some(anchor.saturating_add(inner))
                }
                // A transparent wrapper's children are in both spaces too, each
                // by its own rule, so the walk recurses rather than assuming.
                InlineNode::Hyperlink(link) => {
                    plain_offset_to_anchor_offset(notes, &link.inlines, inner)
                        .map(|within| anchor.saturating_add(within))
                }
                InlineNode::Sdt(sdt) => plain_offset_to_anchor_offset(notes, &sdt.inlines, inner)
                    .map(|within| anchor.saturating_add(within)),
                InlineNode::Revision(revision)
                    if revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup) =>
                {
                    plain_offset_to_anchor_offset(notes, &revision.inlines, inner)
                        .map(|within| anchor.saturating_add(within))
                }
                // A field's anchor length is its own cached-result rule, which is
                // not the plain text of its children; an offset inside one has no
                // translation this function can prove, so it is reported.
                _ => None,
            };
        }
        plain = plain.saturating_add(plain_len);
        anchor = anchor.saturating_add(anchor_len);
    }
    (plain == target).then_some(anchor)
}

/// The removed text of a change, **only when the record carries it verbatim**.
///
/// This is the one place in applying a comparison where getting it wrong invents
/// text in the reader's document. `DiffChange::left_text` is
/// `record::excerpt(…, EXCERPT_BYTES)`: at most 160 bytes of the removed text
/// with `…` appended when it was cut. A tracked deletion carries the removed
/// text in its own runs, so writing an excerpt into one would record a deletion
/// of text the compared document never contained — and Reject would then put
/// that invented text into the document.
///
/// So the excerpt is used only when it provably is not one: its byte length
/// equals the left anchor's span **and** that span is within the excerpt bound.
/// The length test alone is not enough, and that is not theoretical — a removal
/// of exactly 163 bytes cut at byte 160 produces an excerpt of 160 bytes plus
/// the 3 bytes of `…`, whose length equals the span while its last three bytes
/// are invented. The bound closes that collision, because a verbatim excerpt is
/// never longer than the bound.
fn verbatim_removed_text(change: &DiffChange) -> Option<String> {
    let text = change.left_text.as_ref()?;
    let left = change.left.as_ref()?;
    let span = usize::try_from(left.end.saturating_sub(left.start)).unwrap_or(usize::MAX);
    (text.len() == span && span <= VERBATIM_TEXT_BYTES).then(|| text.clone())
}

/// Turns one change into the edit it asks for, or reports why it cannot be one.
///
/// Every `None` has recorded a key first: a comparison that quietly applied
/// three of its five changes and reported "done" is the silent loss `AGENTS.md`
/// forbids.
fn classify_change(
    document: &Document,
    notes: &NoteAnchorLengths,
    change: &DiffChange,
    loss: &mut BTreeSet<&'static str>,
) -> Option<ComparisonEdit> {
    // What this document's own text can be marked as. `UpdateReviewState`
    // replaces the inlines of paragraphs that EXIST, so every expressible change
    // is one whose content is present here: an insertion, a move's destination,
    // or a deletion recorded inside a surviving paragraph.
    let marked = match (change.family, change.kind) {
        (DiffFamily::Text, DiffKind::Insertion) | (DiffFamily::Block, DiffKind::Insertion) => {
            Some(RevisionKind::Insertion)
        }
        (DiffFamily::Block, DiffKind::MoveTo) => Some(RevisionKind::MoveTo),
        (DiffFamily::Text, DiffKind::Deletion) => None,
        // A whole block the compared document has and this one does not has
        // nowhere to be marked: the operation edits a paragraph's inlines and
        // cannot add a paragraph. Reported, not approximated by marking a
        // neighbour.
        (DiffFamily::Block, DiffKind::Deletion) => return unapplied(loss, "blockDeletion"),
        // The far half of a move pair is in the compared document only, for the
        // same reason. `trackedMove` is the key an edit that cannot carry a move
        // already uses.
        (_, DiffKind::MoveFrom) => return unapplied(loss, "trackedMove"),
        (family, _) => return unapplied(loss, family_loss_key(family)),
    };

    let Some(right) = change.right.as_ref() else {
        return unapplied(loss, "unresolvedAnchor");
    };
    // Body only, deliberately. The comparison's right-hand side is a re-export
    // of this document, so a body path maps back by identity; a header, footer,
    // note or comment story is paired by semantic position or by ordinal, which
    // depends on the export writing the same section structure back, and this
    // lane has not measured that. An unmeasured mapping would place a revision
    // in the wrong header rather than refuse to, so the story is reported.
    if right.story != Story::Body {
        return unapplied(loss, "otherStory");
    }
    let Some(block) = block_at_path(document, &right.story, &right.path) else {
        return unapplied(loss, "unresolvedAnchor");
    };
    let BlockNode::Paragraph(paragraph) = block else {
        return unapplied(loss, "nonParagraphBlock");
    };
    let plain = block_text(block).unwrap_or_default();

    // A whole-block insertion's anchor is `0..0` — the change *is* the block —
    // so its extent is the block's own text, read from this document rather than
    // from the record's excerpt of it.
    let (plain_start, plain_end) = if change.family == DiffFamily::Block {
        (0, u32::try_from(plain.len()).unwrap_or(u32::MAX))
    } else {
        (right.start, right.end)
    };
    if plain_start > plain_end || plain_end as usize > plain.len() {
        return unapplied(loss, "unresolvedAnchor");
    }
    let Some(start) = plain_offset_to_anchor_offset(notes, &paragraph.inlines, plain_start) else {
        return unapplied(loss, "offsetSpace");
    };
    let end = if plain_end == plain_start {
        start
    } else {
        match plain_offset_to_anchor_offset(notes, &paragraph.inlines, plain_end) {
            Some(end) => end,
            None => return unapplied(loss, "offsetSpace"),
        }
    };

    let removed = verbatim_removed_text(change);
    if removed.is_none() && change.left_text.is_some() {
        // The record carries the removed text only as an excerpt, so the
        // deletion half of this change is reported rather than invented. The
        // insertion half below is unaffected: it marks text this document
        // already holds.
        loss.insert("truncatedText");
    }
    let marked = match marked {
        Some(kind) if end > start => Some(kind),
        // The range this document was supposed to have gained occupies no
        // anchor offsets — a tab or a break alone, say — so there is nothing to
        // wrap. Nothing is invented and the change is reported.
        Some(_) => {
            loss.insert("insertionNotText");
            None
        }
        None => None,
    };
    if marked.is_none() && removed.is_none() {
        return None;
    }
    Some(ComparisonEdit {
        node: paragraph.id,
        start,
        end,
        marked,
        removed,
    })
}

#[wasm_bindgen]
impl WasmDocument {
    /// Applies a comparison sidecar to this document as tracked changes
    /// (**ADR-061**).
    ///
    /// `sidecar` is `VersionDiff`'s JSON (`casual-doc-diff` `DIFF_SCHEMA` 1)
    /// produced by comparing `left` = the other document against `right` = THIS
    /// document's own exported bytes — [`begin_version_diff`]'s orientation,
    /// which is review's. Each `DiffChange` becomes an `InlineNode::Revision`
    /// authored to `author`/`date`, and the lot is applied as **one**
    /// `Operation::UpdateReviewState` under `HistoryKind::Review`: a single undo
    /// step, after which every existing review surface reads the comparison with
    /// no further change — `listRevisions`, the author-coloured underline and
    /// strikethrough on the canvas, `setShowChanges`, `decideRevision` and
    /// `decideAllRevisions`, next/previous, and `w:ins`/`w:del` on export. The
    /// open document *becomes* the merged document, so "save the result as a new
    /// version" is an ordinary save.
    ///
    /// Refuses, rather than silently merging, when this document already carries
    /// tracked changes: one `reviewType` cannot hold both "a person suggested
    /// this" and "a comparison computed this" without the two deciding each
    /// other. ONLYOFFICE resolves it by accepting every existing change first;
    /// destroying a reviewer's suggestions to run a comparison is the loss
    /// `docs/158` §2.4 puts first, so this refuses with its own sentence.
    ///
    /// What a tracked change cannot say is **reported** through
    /// `EditResult::pasteLoss` — a whole block the other document has and this
    /// one does not, a move's far half, a
    /// formatting/style/section/definition/resource/comment/metadata difference
    /// (our four `RevisionKind`s have no form for them), a story outside the
    /// body, removed text the record carries only as an excerpt, and a path that
    /// no longer resolves. The field's name reads oddly for a comparison and is
    /// used anyway: its own contract is that the next path to degrade something
    /// must not invent a second channel.
    ///
    /// Complexity: **O(changes + the paragraphs they touch)**, plus the one
    /// paragraph index the review operation builds. Not O(document) per change.
    ///
    /// # Errors
    ///
    /// Throws when the sidecar is unreadable or carries another schema, when the
    /// author name is missing or too long, and when this document already
    /// carries tracked changes.
    #[wasm_bindgen(js_name = applyDiffAsRevisions)]
    pub fn apply_diff_as_revisions(
        &mut self,
        sidecar: &str,
        author: &str,
        date: Option<String>,
    ) -> Result<EditResult, JsValue> {
        self.apply_diff_as_revisions_inner(sidecar, author, date)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// `apply_diff_as_revisions` without the `JsValue`, so native tests exercise
    /// the path the boundary does.
    pub(crate) fn apply_diff_as_revisions_inner(
        &mut self,
        sidecar: &str,
        author: &str,
        date: Option<String>,
    ) -> Result<EditResult, String> {
        let diff: VersionDiff = serde_json::from_str(sidecar).map_err(|error| {
            casual_doc_edit::refusal::marked(
                "compare.sidecar-unreadable",
                &format!("This comparison could not be read ({error})."),
            )
        })?;
        // A host that does not recognise the schema must refuse rather than
        // render a partial diff (`record::DIFF_SCHEMA`); applying one is the same
        // rule with teeth, because a field that moved between schemas would mark
        // the wrong text.
        if diff.schema != DIFF_SCHEMA {
            return Err(casual_doc_edit::refusal::marked(
                "compare.schema-unsupported",
                &format!(
                    "This comparison was made by a different version of the editor (format {}; this one reads {DIFF_SCHEMA}).",
                    diff.schema
                ),
            ));
        }
        validate_authored_revision_author(Some(author))?;
        // The same three collections `decideAllRevisions` uses to decide that a
        // document has NO tracked changes, so the refusal and Accept All cannot
        // disagree about what carries one.
        let mut existing = Vec::new();
        collect_review_revision_ids_all(&self.document, &mut existing);
        let mut existing_formats = Vec::new();
        collect_review_format_ids_all(&self.document, &mut existing_formats);
        let existing_paragraphs = collect_paragraph_revisions_all(&self.document);
        if !existing.is_empty() || !existing_formats.is_empty() || !existing_paragraphs.is_empty() {
            return Err(refused!(
                "compare.document-has-revisions",
                "This document already has tracked changes. Accept or reject them first — \
                 a comparison records its own tracked changes, and merging the two would \
                 decide someone else's suggestions for them."
            )
            .to_string());
        }

        let notes = NoteAnchorLengths::of(&self.document);
        let mut loss: BTreeSet<&'static str> = BTreeSet::new();
        // The comparison itself stopped short of being exhaustive, so reporting
        // it as applied without saying so would overstate the result.
        if !diff.complete {
            loss.insert("incompleteComparison");
        }
        let mut edits: Vec<ComparisonEdit> = Vec::new();
        for change in &diff.changes {
            if let Some(edit) = classify_change(&self.document, &notes, change, &mut loss) {
                edits.push(edit);
            }
        }
        let caret = edits.first().map_or_else(
            || Pos::new(self.document.id(), 0),
            |edit| Pos::new(edit.node, edit.start),
        );

        // Grouped by paragraph, and applied within one in DESCENDING offset
        // order: a recorded deletion inserts the removed text as new inlines, so
        // working from the end means no later edit's offsets have moved by the
        // time it is applied.
        let mut by_node: BTreeMap<NodeId, Vec<ComparisonEdit>> = BTreeMap::new();
        for edit in edits {
            by_node.entry(edit.node).or_default().push(edit);
        }
        // ONE index for every paragraph the comparison touches, not one scan per
        // paragraph. `review_paragraph_body` — what every `suggest*` method uses
        // — resolves its one id with `find_paragraph_any`, which walks every
        // surface; that is right for a keystroke and quadratic here, where a
        // comparison can touch hundreds of paragraphs (`104` HF-111, `116`). The
        // index is built, read, and dropped before the mutation starts, because
        // it borrows the document the edits are about to change.
        let mut planned: Vec<(Vec<ComparisonEdit>, Vec<BlockNode>)> = Vec::new();
        {
            let by_id = ParagraphIndex::build(&self.document);
            for (node, mut group) in by_node {
                let Some(paragraph) = by_id.paragraph(node) else {
                    // The path resolved to this paragraph a moment ago, so this
                    // is unreachable rather than tolerated — but reported, not
                    // unwrapped.
                    loss.insert("unresolvedAnchor");
                    continue;
                };
                group.sort_by(|a, b| b.start.cmp(&a.start));
                planned.push((group, vec![BlockNode::Paragraph(paragraph.clone())]));
            }
        }
        let mut bodies: Vec<Vec<BlockNode>> = Vec::new();
        for (group, mut body) in planned {
            for edit in group {
                self.apply_comparison_edit(&notes, &mut body, &edit, author, &date, &mut loss)?;
            }
            bodies.push(body);
        }

        // ONE operation for the whole comparison, so accepting it, rejecting it
        // and undoing it are each a single act.
        let operation = match update_review_operation_across(&self.document, &bodies, None) {
            Ok(operation) => operation,
            // Nothing in the comparison could be expressed. The document is
            // reported UNCHANGED rather than routed through an empty edit, which
            // would bump the revision and enable Save for a comparison that
            // changed nothing — and `loss` says why.
            Err(_) => {
                return Ok(EditResult {
                    node: caret.node.to_string(),
                    offset: caret.offset,
                    revision: self.revision,
                    page_count: self.page_count(),
                    dirty: Vec::new(),
                    paste_loss: loss.into_iter().map(str::to_owned).collect(),
                });
            }
        };
        let mut result = self.apply_action_caret_as(vec![operation], caret, HistoryKind::Review)?;
        result.paste_loss = loss.into_iter().map(str::to_owned).collect();
        Ok(result)
    }

    /// One change's share of the review body: the insertion wrapper, then the
    /// recorded deletion before it.
    ///
    /// The two are written in that order so the inline sequence ends up
    /// `[deletion][insertion]`, which is exactly `RevisionGroupKind::Replacement`
    /// — one deletion followed by one insertion, contiguous, same author and
    /// date — and so decides as one card.
    fn apply_comparison_edit(
        &mut self,
        notes: &NoteAnchorLengths,
        body: &mut Vec<BlockNode>,
        edit: &ComparisonEdit,
        author: &str,
        date: &Option<String>,
        loss: &mut BTreeSet<&'static str>,
    ) -> Result<(), String> {
        let group = match (edit.marked, edit.removed.as_ref()) {
            (Some(_), Some(_)) => Some(RevisionGroup {
                id: self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?,
                kind: RevisionGroupKind::Replacement,
            }),
            _ => None,
        };
        if let Some(kind) = edit.marked {
            let revision = Revision {
                id: self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?,
                kind,
                author: Some(author.to_owned()),
                date: date.clone(),
                revision_id: Some(self.revision_ids.allocate()?),
                editor_group: group,
                inlines: Vec::new(),
            };
            // Misnamed by history: it wraps a range in whatever revision it is
            // given, which is how a comparison's INSERTION is marked with it.
            if !wrap_review_deletion(
                notes,
                body,
                edit.node,
                edit.start,
                edit.end,
                revision,
                &mut self.edit_ids,
            ) {
                loss.insert("notParagraphText");
                return Ok(());
            }
        }
        if let Some(text) = edit.removed.clone() {
            let run = self
                .edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?;
            let revision = Revision {
                id: self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?,
                kind: RevisionKind::Deletion,
                author: Some(author.to_owned()),
                date: date.clone(),
                revision_id: Some(self.revision_ids.allocate()?),
                editor_group: group,
                inlines: vec![InlineNode::Run(Run {
                    id: run,
                    properties: RunProperties::default().into(),
                    text,
                })],
            };
            if !insert_review_revision(
                notes,
                body,
                edit.node,
                edit.start,
                revision,
                &mut self.edit_ids,
            ) {
                loss.insert("notParagraphText");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DOCX fixture that carries a media part, so the digest path is exercised
    /// against a real package rather than a constructed one.
    const PICTURE_DOCX: &[u8] =
        include_bytes!("../../../fixtures/generated/picture-hyperlink.docx");

    /// A DOCX the importer refuses.
    const TRUNCATED_DOCX: &[u8] =
        include_bytes!("../../../fixtures/generated/malformed-truncated.docx");

    /// Plain text is a registered format, so a fixture needs no ZIP.
    fn text(lines: &[&str]) -> Vec<u8> {
        lines.join("\n").into_bytes()
    }

    /// The facade produces the same sidecar whether it is driven in slices or run
    /// in one call. If it did not, the budgeted path would be a second
    /// implementation of the diff with its own bugs.
    #[test]
    fn a_sliced_diff_and_a_blocking_diff_agree() {
        let left = text(&["alpha", "beta", "gamma"]);
        let right = text(&["alpha", "beta edited", "gamma"]);
        let blocking = diff_versions_inner(&left, &right).expect("the blocking path works");

        let mut job = begin_version_diff(left, right);
        let mut states = Vec::new();
        for _ in 0..10_000 {
            let state = job.step_inner(1).expect("no step fails");
            states.push(state.clone());
            if state == PHASE_COMPLETE {
                break;
            }
        }
        assert_eq!(
            states
                .iter()
                .filter(|state| *state == PHASE_PARSING)
                .count(),
            2,
            "one parsing slice per side; got {states:?}"
        );
        assert!(
            states
                .iter()
                .filter(|state| *state == PHASE_WORKING)
                .count()
                > 1,
            "a one-block budget took several working slices; got {states:?}"
        );
        let sliced = job.result().expect("a sliced result");
        // Everything but the step counter, which is a diagnostic ABOUT the
        // driving and is meant to differ: several one-block slices against one
        // blocking call.
        let strip = |json: &str| {
            let mut value: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
            value["diagnostics"]["steps"] = serde_json::Value::Null;
            value
        };
        assert_eq!(
            strip(&sliced),
            strip(&blocking),
            "the sliced path and the blocking path agree, change for change"
        );
        assert!(
            sliced.contains("\"family\":\"text\""),
            "and the agreed-on answer is the edited paragraph: {sliced}"
        );
    }

    /// The other way node identity fails as a match key, and the reason the diff
    /// never uses it as one.
    ///
    /// A DOCX parse numbers from one in a fixed namespace, so two checkpoints hand
    /// out the SAME ids to different paragraphs (`casual-doc-diff`'s own guard
    /// shows that). Plain text derives its namespace from a hash of the whole text
    /// (`casual_doc_io::text`), so two different text checkpoints share NO ids at
    /// all — matching on them would report every paragraph as deleted and re-added.
    /// Opposite failures, same conclusion.
    #[test]
    fn plain_text_checkpoints_share_no_node_ids_at_all() {
        let first = import_for_diff(&text(&["alpha", "beta"])).expect("opens");
        let second = import_for_diff(&text(&["alpha", "gamma"])).expect("opens");
        let ids = |document: &Document| -> Vec<u128> {
            document
                .body()
                .iter()
                .map(|block| match block {
                    BlockNode::Paragraph(paragraph) => paragraph.id.as_u128(),
                    _ => 0,
                })
                .collect()
        };
        let (left, right) = (ids(&first.document), ids(&second.document));
        assert!(!left.is_empty() && left.len() == right.len());
        assert!(
            left.iter().all(|id| !right.contains(id)),
            "two text checkpoints share no node id: {left:?} against {right:?}"
        );
        // And the diff still finds the one edited paragraph, without any shared id.
        let json = diff_versions_inner(&text(&["alpha", "beta"]), &text(&["alpha", "gamma"]))
            .expect("both open");
        assert!(
            json.contains("\"family\":\"text\""),
            "content alignment found the edit without any shared id: {json}"
        );
    }

    /// The progress denominator becomes known only once both sides are parsed, and
    /// the count reaches it — so a determinate bar ends at the end.
    #[test]
    fn the_block_total_is_known_after_both_parses_and_is_reached() {
        let mut job = begin_version_diff(text(&["one", "two"]), text(&["one", "three"]));
        assert_eq!(job.blocks_total(), 0.0, "nothing is known before parsing");
        assert_eq!(job.step_inner(1).unwrap(), PHASE_PARSING);
        assert_eq!(
            job.blocks_total(),
            0.0,
            "and nothing is claimed after only one side"
        );
        assert_eq!(job.step_inner(1).unwrap(), PHASE_PARSING);
        let total = job.blocks_total();
        assert_eq!(total, 4.0, "two paragraphs a side");
        while job.step_inner(1_000).unwrap() == PHASE_WORKING {}
        assert_eq!(
            job.blocks_projected(),
            total,
            "the bar reaches its end rather than stopping short"
        );
    }

    /// Cancelling stops the job and yields nothing, however often a host keeps
    /// stepping it.
    #[test]
    fn a_cancelled_diff_yields_nothing_however_often_it_is_stepped() {
        let mut job = begin_version_diff(text(&["a"]), text(&["b"]));
        assert_eq!(job.step_inner(1).unwrap(), PHASE_PARSING);
        job.cancel();
        for _ in 0..5 {
            assert_eq!(
                job.step_inner(1_000_000).unwrap(),
                PHASE_CANCELLED,
                "a cancelled job stays cancelled"
            );
        }
        assert!(job.result().is_none(), "and produces nothing");
    }

    /// A checkpoint the engine cannot open is REPORTED. A diff that quietly
    /// compared it as an empty document would tell a reader their whole document
    /// had been deleted.
    #[test]
    fn an_unopenable_checkpoint_is_reported_rather_than_compared_as_empty() {
        let error = diff_versions_inner(TRUNCATED_DOCX, PICTURE_DOCX)
            .expect_err("a refusal, not an empty diff");
        assert!(
            error.contains("import version"),
            "the refusal says which side and what happened; got {error}"
        );
    }

    /// The facade computes media digests from the imported resources, which is the
    /// half of resource comparison the engine crate cannot do: a `Document` models
    /// a media *reference*, not its bytes. Without them every document with media
    /// would carry a `missing_resource` finding and could never call itself a
    /// complete diff.
    #[test]
    fn the_facade_supplies_media_digests_so_no_resource_gap_is_reported() {
        let side = import_for_diff(PICTURE_DOCX).expect("the fixture opens");
        assert!(
            !side.digests.is_empty(),
            "the fixture really does carry a media part, so this guard can fail"
        );
        let json = diff_versions_inner(PICTURE_DOCX, PICTURE_DOCX).expect("both open");
        let diff: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(
            !diff["findings"]
                .as_array()
                .expect("an array")
                .iter()
                .any(|finding| finding["construct"] == "mediaBytes"),
            "no missing-resource finding, because the digests were supplied; got {:?}",
            diff["findings"]
        );
        assert_eq!(
            diff["complete"], true,
            "and a document with media can therefore be a complete diff"
        );
    }

    /// The default slice is a block count, not a duration, and a host with no
    /// measurement can use it.
    #[test]
    fn the_default_slice_finishes_a_small_diff() {
        let mut job = begin_version_diff(text(&["a", "b"]), text(&["a", "c"]));
        let mut states = 0;
        loop {
            states += 1;
            assert!(states < 100, "a small diff finishes in a few slices");
            // Through the exported getter, not the constant, so the entry point a
            // host actually reads is the one under test.
            if job.step_inner(default_diff_slice()).unwrap() == PHASE_COMPLETE {
                break;
            }
        }
        let json = job.result().expect("a result");
        assert!(
            json.contains("\"schema\":1"),
            "the sidecar is versioned: {json}"
        );
    }

    // =========================================================================
    // Applying a comparison as tracked changes (ADR-061).
    // =========================================================================

    /// A document and a comparison of it against `older`, in the orientation
    /// `applyDiffAsRevisions` documents: `left` is the other document, `right` is
    /// this one's own bytes.
    fn open_and_compare(older: &[u8], live: &[u8]) -> (crate::WasmDocument, String) {
        let document = crate::open_document(live).expect("the live document opens");
        let sidecar = diff_versions_inner(older, live).expect("both sides open");
        (document, sidecar)
    }

    /// The typed revision list the review panel, the gutter and next/previous all
    /// read.
    fn revisions_of(document: &crate::WasmDocument) -> Vec<serde_json::Value> {
        serde_json::from_str(&document.list_revisions()).expect("a typed revision list")
    }

    /// The document as rejecting every tracked change would leave it: the
    /// `Original` projection, which is where a deletion own runs contribute and
    /// so where text invented by a deletion would land.
    fn original_text(document: &crate::WasmDocument) -> String {
        let mut text = String::new();
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                append_node_plain_text(&paragraph.inlines, ReviewProjection::Original, &mut text);
            }
        }
        text
    }

    /// Whether a rendered page carries **coloured** ink — a pixel whose channels
    /// are not all equal.
    ///
    /// Plain text paints black on white, so every pixel of an unmarked page is
    /// grey. Review markup paints an insertion in the author's hue
    /// (`flow::review_author_color`), so coloured ink on the page *is* the markup
    /// having been painted. Measured from pixels rather than from the model,
    /// because "the model holds a revision" is what the rest of these guards
    /// already assert, and `105` CQ-003's lesson is that a construct can be typed,
    /// anchored and listed while the reader still sees nothing.
    fn has_colored_ink(bitmap: &crate::PageBitmap) -> bool {
        bitmap.rgba.chunks_exact(4).any(|pixel| {
            pixel[3] > 0
                && (pixel[0].abs_diff(pixel[1]) > 24
                    || pixel[1].abs_diff(pixel[2]) > 24
                    || pixel[0].abs_diff(pixel[2]) > 24)
        })
    }

    /// A diff offset is NOT a review offset, and the difference is a tab.
    ///
    /// `DiffAnchor` claimed the two spaces were the same; they agree for runs and
    /// symbols and disagree for everything whose projected text and anchor length
    /// differ. A tab is one byte of projected text and zero anchor bytes, so
    /// applying a diff offset directly would mark the wrong characters in any
    /// paragraph with a tab in it - which is most of the ones that look tabular.
    #[test]
    fn a_diff_offset_is_translated_into_the_review_offset_space() {
        let run = |id: u128, text: &str| {
            InlineNode::Run(Run {
                id: casual_doc_model::NodeId::new(id).expect("a non-zero id"),
                properties: RunProperties::default().into(),
                text: text.to_owned(),
            })
        };
        let inlines = vec![
            run(1, "ab"),
            InlineNode::Tab(casual_doc_model::v1::Tab {
                id: casual_doc_model::NodeId::new(2).expect("a non-zero id"),
            }),
            run(3, "cd"),
        ];
        let notes = NoteAnchorLengths::default();
        let mut plain = String::new();
        append_node_plain_text(&inlines, ReviewProjection::FinalWithMarkup, &mut plain);
        assert_eq!(
            plain, "ab\tcd",
            "the precondition: the tab is a byte of plain text"
        );
        assert_eq!(
            crate::inlines_anchor_len(&notes, &inlines),
            4,
            "and none of the anchor space, so the two spaces really do differ here"
        );

        for (diff_offset, review_offset) in [(0, 0), (2, 2), (3, 2), (5, 4)] {
            assert_eq!(
                plain_offset_to_anchor_offset(&notes, &inlines, diff_offset),
                Some(review_offset),
                "plain offset {diff_offset} is review offset {review_offset}"
            );
        }
    }

    /// The moment Compare stops being a count: one text edit becomes one tracked
    /// insertion that `listRevisions` reports and that the markup render paints in
    /// the author's colour.
    #[test]
    fn one_text_edit_becomes_one_revision_the_review_surface_lists_and_the_canvas_paints() {
        let older = text(&["alpha", "beta"]);
        let live = text(&["alpha", "beta edited"]);
        let (mut document, sidecar) = open_and_compare(&older, &live);
        assert!(
            sidecar.contains("\"family\":\"text\""),
            "the precondition: the comparison really did find a text edit: {sidecar}"
        );
        assert!(
            revisions_of(&document).is_empty(),
            "and the document starts with no tracked changes"
        );
        assert!(
            !has_colored_ink(
                &document
                    .render_page_inner(0, 96.0)
                    .expect("the page renders")
            ),
            "the precondition for the paint assertion: an unmarked page is black on white"
        );

        let result = document
            .apply_diff_as_revisions_inner(
                &sidecar,
                "Compared document",
                Some("2026-10-04T00:00:00Z".to_owned()),
            )
            .expect("the comparison applies");
        assert!(
            result.paste_loss().is_empty(),
            "a plain text edit degrades nothing, so nothing is reported: {:?}",
            result.paste_loss()
        );

        let revisions = revisions_of(&document);
        assert_eq!(
            revisions.len(),
            1,
            "one text edit is one tracked change: {revisions:?}"
        );
        assert_eq!(revisions[0]["kind"], "insertion");
        assert_eq!(revisions[0]["author"], "Compared document");
        assert_eq!(revisions[0]["date"], "2026-10-04T00:00:00Z");
        assert!(
            revisions[0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("edited")),
            "and it is the edited text, not a count: {revisions:?}"
        );

        document
            .set_show_changes_inner(true)
            .expect("the markup preview builds");
        assert!(
            has_colored_ink(
                &document
                    .render_page_inner(0, 96.0)
                    .expect("the markup page renders")
            ),
            "the comparison is painted on the canvas in the author's colour"
        );
    }

    /// A comparison is ONE undo step however many paragraphs it touches, because
    /// it is one `UpdateReviewState`. A reader who undoes a comparison means the
    /// whole comparison.
    #[test]
    fn the_whole_comparison_is_one_undo_step() {
        let older = text(&["alpha", "beta", "gamma", "delta"]);
        let live = text(&["alpha one", "beta two", "gamma three", "delta four"]);
        let (mut document, sidecar) = open_and_compare(&older, &live);
        let before = document.revision;

        document
            .apply_diff_as_revisions_inner(&sidecar, "Compared document", None)
            .expect("the comparison applies");
        let applied = revisions_of(&document);
        assert!(
            applied.len() >= 4,
            "the precondition: several paragraphs carry a change, so one undo step is \
             a claim worth making: {applied:?}"
        );
        assert_eq!(
            document.revision,
            before + 1,
            "four paragraphs, one model revision"
        );

        document.undo_inner().expect("one undo");
        assert!(
            revisions_of(&document).is_empty(),
            "a single undo removes the WHOLE comparison, not its last paragraph"
        );
        assert_eq!(
            document.undo_label(),
            "",
            "and there is no second half of the comparison left to undo"
        );
    }

    /// A document that already carries a reviewer's suggestions is refused, with
    /// its own sentence and its own routing code — never the host's generic "that
    /// edit isn't supported for this selection yet", and never by accepting the
    /// reviewer's changes first the way ONLYOFFICE does (`docs/158` §2.4).
    #[test]
    fn a_document_already_carrying_tracked_changes_is_refused_with_its_own_sentence() {
        let older = text(&["alpha", "beta"]);
        let live = text(&["alpha", "beta edited"]);
        let (mut document, sidecar) = open_and_compare(&older, &live);
        let node = match &document.document.body()[0] {
            BlockNode::Paragraph(paragraph) => paragraph.id.to_string(),
            other => panic!("the fixture's first block is a paragraph, got {other:?}"),
        };
        document
            .suggest_insert(&node, 0, "Reviewed", Some("Ada".to_owned()), None, None)
            .expect("a reviewer suggests something first");

        let refusal = document
            .apply_diff_as_revisions_inner(&sidecar, "Compared document", None)
            .expect_err("a refusal, not a merge");
        let (sentence, code) = casual_doc_edit::refusal::split(&refusal);
        assert_eq!(
            code,
            Some("compare.document-has-revisions"),
            "the refusal carries its own routing code so a host can translate it: {refusal:?}"
        );
        assert!(
            sentence.contains("already has tracked changes"),
            "and its own sentence, not a generic one: {sentence:?}"
        );
        assert_eq!(
            revisions_of(&document).len(),
            1,
            "and the reviewer's suggestion is still there, untouched"
        );
    }

    /// An unreadable sidecar and a sidecar from another schema are both refused
    /// with their own codes. A schema this build does not know may have moved a
    /// field, and applying it would mark the wrong text.
    #[test]
    fn an_unreadable_or_foreign_sidecar_is_refused_with_its_own_code() {
        let live = text(&["alpha"]);
        let mut document = crate::open_document(&live).expect("opens");

        let unreadable = document
            .apply_diff_as_revisions_inner("{\"not\":\"a sidecar\"}", "Compared document", None)
            .expect_err("a refusal");
        assert_eq!(
            casual_doc_edit::refusal::split(&unreadable).1,
            Some("compare.sidecar-unreadable")
        );

        let sidecar = diff_versions_inner(&text(&["alpha", "beta"]), &live).expect("both open");
        let mut foreign: serde_json::Value = serde_json::from_str(&sidecar).expect("valid JSON");
        foreign["schema"] = serde_json::json!(DIFF_SCHEMA + 1);
        let refused = document
            .apply_diff_as_revisions_inner(&foreign.to_string(), "Compared document", None)
            .expect_err("a refusal");
        assert_eq!(
            casual_doc_edit::refusal::split(&refused).1,
            Some("compare.schema-unsupported")
        );
        assert!(
            revisions_of(&document).is_empty(),
            "and neither refusal left half a comparison in the document"
        );
    }

    /// **The data-loss case.** `DiffChange::left_text` is a 160-byte excerpt with
    /// `…` appended, and a tracked deletion carries the removed text verbatim in
    /// its own runs — so applying the excerpt would record a deletion of text the
    /// compared document never contained, and Reject would then write that
    /// invented text into the reader's document. A removal the record does not
    /// carry verbatim is REPORTED and not applied.
    #[test]
    fn a_truncated_removal_is_reported_rather_than_invented() {
        // Long enough that the removed run exceeds the excerpt bound, with a
        // common prefix long enough that the two paragraphs still pair as one
        // edited paragraph rather than a delete plus an add.
        let common = "the quick brown fox jumps over the lazy dog ".repeat(8);
        let removed = "x".repeat(300);
        let older = format!("alpha\n{common}{removed}").into_bytes();
        let live = format!("alpha\n{common}").into_bytes();
        let (mut document, sidecar) = open_and_compare(&older, &live);
        let parsed: serde_json::Value = serde_json::from_str(&sidecar).expect("valid JSON");
        let changes = parsed["changes"].as_array().expect("an array");
        assert!(
            changes.iter().any(|change| {
                change["family"] == "text"
                    && change["leftText"]
                        .as_str()
                        .is_some_and(|text| text.ends_with('\u{2026}'))
            }),
            "the precondition: the comparison really did truncate the removed text, \
             so this guard can fail: {changes:?}"
        );

        let result = document
            .apply_diff_as_revisions_inner(&sidecar, "Compared document", None)
            .expect("the comparison is applied as far as it can be");
        // The invention is asserted BEFORE the reporting, so a mutation that
        // applies the excerpt anyway shows the invented text in its failure
        // rather than only an empty loss list.
        let listed = document.list_revisions();
        assert!(
            !listed.contains('\u{2026}'),
            "no tracked change carries the excerpt's ellipsis, i.e. nothing was \
             invented: {listed}"
        );
        let rejected = original_text(&document);
        assert!(
            !rejected.contains('\u{2026}'),
            "nor would rejecting every change write it into the document: {rejected}"
        );
        assert!(
            result.paste_loss().contains(&"truncatedText".to_owned()),
            "and the removal that could not be applied is reported: {:?}",
            result.paste_loss()
        );
    }

    /// The families a comparison can detect but tracked changes cannot say are
    /// reported through `pasteLoss`, never swallowed. A comparison that applied
    /// three of its five changes and reported success is the silent loss
    /// `AGENTS.md` forbids.
    #[test]
    fn what_tracked_changes_cannot_express_is_reported_not_swallowed() {
        // A removed paragraph (no paragraph in this document to mark) and a
        // changed paragraph beside it.
        let older = text(&["alpha", "removed entirely", "gamma"]);
        let live = text(&["alpha", "gamma edited"]);
        let (mut document, sidecar) = open_and_compare(&older, &live);

        let result = document
            .apply_diff_as_revisions_inner(&sidecar, "Compared document", None)
            .expect("the comparison applies what it can");
        assert!(
            result.paste_loss().contains(&"blockDeletion".to_owned()),
            "a whole block this document does not have is reported: {:?}",
            result.paste_loss()
        );
        assert!(
            !revisions_of(&document).is_empty(),
            "while the changes that CAN be expressed are still applied"
        );
    }

    /// Cost is **O(changes)**, not O(changes × document): the number of
    /// whole-document scans must not grow with the number of changes.
    ///
    /// Measured with `document_scans`, not a clock — `116`'s whole point is that a
    /// timing test cannot tell a slow constant from one scan per node, and the
    /// defect this guards against is exactly that shape: `review_paragraph_body`,
    /// which every `suggest*` method uses, resolves its id with
    /// `find_paragraph_any`, and calling it once per touched paragraph would make
    /// a 400-change comparison walk the document 400 times.
    #[test]
    fn applying_a_comparison_scans_the_document_a_fixed_number_of_times() {
        /// One document of `total` paragraphs, of which the first `edited` differ
        /// from the compared side. The document is the SAME size both times, so
        /// only the change count varies.
        fn scans_for(total: usize, edited: usize) -> (usize, usize) {
            let live: Vec<String> = (0..total).map(|index| format!("line {index}")).collect();
            let older: Vec<String> = live
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    if index < edited {
                        format!("{line} as it was")
                    } else {
                        line.clone()
                    }
                })
                .collect();
            let live = live.join("\n").into_bytes();
            let older = older.join("\n").into_bytes();
            let (mut document, sidecar) = open_and_compare(&older, &live);
            casual_doc_edit::reset_document_scans();
            document
                .apply_diff_as_revisions_inner(&sidecar, "Compared document", None)
                .expect("the comparison applies");
            (
                casual_doc_edit::document_scans(),
                revisions_of(&document).len(),
            )
        }

        let (few_scans, few_changes) = scans_for(48, 6);
        let (many_scans, many_changes) = scans_for(48, 24);
        assert!(
            many_changes >= few_changes * 2,
            "the precondition: the change count really did more than double \
             ({few_changes} then {many_changes})"
        );
        assert_eq!(
            few_scans, many_scans,
            "four times the changes in the same document must cost the same number of \
             whole-document scans ({few_scans} then {many_scans})"
        );
    }
}
