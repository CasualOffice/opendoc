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

use casual_doc_diff::{DiffJob, DiffSides, MediaDigests, Progress};
use casual_doc_io::{DetectionRequest, FormatSelection, builtin_registry_with_limits};
use casual_doc_model::v1::{BlockNode, Document};
use wasm_bindgen::prelude::*;

use crate::{to_js, viewer_limits, viewer_text_limits};

/// The default slice size a host that has no measurement yet can pass.
///
/// Blocks, not milliseconds: a millisecond budget cannot be honoured by a call
/// that has already started, and the caller is the only one who knows what its
/// frame costs. `background_measure.mjs` converges on the block count that fits
/// its own budget; this is the value to start that convergence from.
pub const DEFAULT_DIFF_SLICE: u32 = 4_000;

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
            if job.step_inner(DEFAULT_DIFF_SLICE).unwrap() == PHASE_COMPLETE {
                break;
            }
        }
        let json = job.result().expect("a result");
        assert!(
            json.contains("\"schema\":1"),
            "the sidecar is versioned: {json}"
        );
    }
}
