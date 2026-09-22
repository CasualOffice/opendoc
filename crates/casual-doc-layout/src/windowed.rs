//! Windowed layout — laying out only the pages someone is looking at.
//!
//! `docs/113` §6 steps 4 and 5. Steps 2 and 3 built the two halves this module
//! joins: a **measure tier** that knows every page boundary without carrying a
//! glyph, and a **resumable paginator** that can start at a checkpoint instead
//! of at page one. What was missing was a driver that (a) builds the measure
//! tier without the shaped galley ever existing, and (b) materializes the paint
//! tier for one window of pages.
//!
//! ```text
//!  open                          scroll
//!  ────                          ──────
//!  measure_document              window_of
//!    build_measures_for_blocks     nearest checkpoint at/ before the window
//!      (MeasureSink: 2 shaped        re-flow only the blocks the window needs
//!       paragraphs live at once)     paginate_from → pages, absolute numbers
//!    paginate_measures            evict paint fragments past the byte budget
//!      → every page boundary
//!      → checkpoints every K pages
//! ```
//!
//! ## What must stay true
//!
//! > A windowed page equals the same page of a full
//! > [`paginate_document`](crate::document_layout::paginate_document), field
//! > for field.
//!
//! There is exactly one paginator (generic over
//! [`Paginable`](crate::measure::Paginable)) and exactly one flow engine
//! (generic over [`GalleySink`](crate::measure::GalleySink)), so a windowed
//! page cannot be produced by a different algorithm — only, potentially, by the
//! same algorithm started from the wrong state. That is what
//! [`FlowResume`] is about, and what `tests/windowed_document.rs` asserts.
//!
//! ## What this refuses, and why refusing is the honest answer
//!
//! [`measure_document`] returns [`NotWindowable`] rather than guessing whenever
//! the driver's *post-pagination* passes can move a page boundary, because the
//! measure tier runs below them (`docs/113` §7):
//!
//! | refused | because |
//! | --- | --- |
//! | more than one section | pages are assembled by [`crate::columns`] across section runs, not by one paginator over one galley |
//! | a multi-column section | same |
//! | body footnotes | `paginate_section_footnotes` reserves a per-page band, so the content height the measure pass used is not the height the page had |
//! | `w:numRestart="eachPage"` notes | a bounded fixed point over *all* pages re-flows the body; `paginate_document_cached` already takes this same fallback |
//! | an anchored float anywhere | `finish_pagination` re-flows against computed exclusions (which changes line breaking and therefore page boundaries), and `place_floats` carries a document-global z-order |
//! | margin line numbering | `place_line_numbers` runs a counter across pages |
//!
//! Each refusal is a fallback to
//! [`paginate_document`](crate::document_layout::paginate_document), which is correct and
//! merely expensive. None of them is silent.

use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::NoteId;
use casual_doc_model::v1::NoteKind;

use crate::anchor::document_has_anchored_object;
use crate::document_layout::SectionPlan;
use crate::document_layout::apply_page_vertical_alignment;
use crate::document_layout::blocks_with_endnotes;
use crate::document_layout::build_section_plans;
use crate::document_layout::mirrored_page_config;
use crate::document_layout::referenced_endnotes;
use crate::flow::MeasureResume;
use crate::flow::NoteFlow;
use crate::flow::ReviewView;
use crate::flow::build_measures_for_blocks_resumed;
use crate::flow::flow_body_range;
use crate::flow::single_section_line_grid;
use crate::incremental::PageRange;
use crate::incremental::ViewportLayout;
use crate::incremental::VisiblePage;
use crate::measure::FragmentMeasure;
use crate::measure::PageOutline;
use crate::note_numbering::NoteLabels;
use crate::note_numbering::resolve_note_labels;
use crate::note_numbering::visit_block_note_refs;
use crate::page::Page;
use crate::page_border::resolve_page_borders;
use crate::paginate::Checkpoint;
use crate::paginate::DEFAULT_CHECKPOINT_INTERVAL;
use crate::paginate::PageConfig;
use crate::paginate::page_number_label_at;
use crate::paginate::paginate_from_based;
use crate::paginate::paginate_measures;
use crate::paginate::paginate_measures_from;
use crate::paginate::resolve_fields_labeled_with_total;
use crate::running::place_running_content_on_page;
use crate::text::LineShaper;
use crate::units::Twip;

/// Why a document cannot be laid out through the windowed path.
///
/// Every variant names a driver pass that runs *after* pagination and can move
/// a page boundary, which is precisely what the measure tier cannot see. The
/// caller's answer to all of them is the same: use
/// [`paginate_document`](crate::document_layout::paginate_document).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NotWindowable {
    /// The document declares more than one section, so pages are assembled
    /// across section runs by [`crate::columns`].
    MultipleSections,
    /// The (single) section is multi-column.
    MultipleColumns,
    /// The body references a footnote, whose page-local band changes the
    /// content height pagination fills.
    BodyFootnotes,
    /// Notes restart numbering each page (`w:numRestart="eachPage"`), which
    /// needs the pagination fixed point in `paginate_document_view`.
    NotesRestartEachPage,
    /// A body paragraph anchors a floating object, so `finish_pagination`
    /// re-flows the body against computed exclusions.
    AnchoredFloats,
    /// The section declares margin line numbering (`w:lnNumType`), whose
    /// counter runs across pages.
    LineNumbering,
}

impl NotWindowable {
    /// A one-line explanation, for a log line or a refusal message.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::MultipleSections => "the document has more than one section",
            Self::MultipleColumns => "the section is multi-column",
            Self::BodyFootnotes => "the body references footnotes",
            Self::NotesRestartEachPage => "notes restart numbering on each page",
            Self::AnchoredFloats => "the document anchors a floating object",
            Self::LineNumbering => "the section numbers lines in the margin",
        }
    }
}

/// Where a window's paint tier may start re-flowing from.
///
/// Flowing a block sequence is a forward walk that carries state: list
/// counters, the previous paragraph's style for `w:contextualSpacing`, wrap
/// carries from a float, and the drop-cap pairing that couples two adjacent
/// paragraphs. Start the walk in the middle and a fragment can come out
/// different from the one a full build produced — a numbered paragraph
/// restarting at 1 is the obvious case.
///
/// Rather than snapshot that state (every field of which is a place for the
/// two paths to drift), the driver **classifies the document** by whether any
/// of it exists at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowResume {
    /// Nothing in the body carries flow state across a block boundary, so
    /// flowing from any block reproduces exactly the fragments a full build
    /// produced. A window costs `O(window)`.
    AnyBlock,
    /// Something does, so a window is flowed from block zero with a sink that
    /// retains only the fragments the window needs. Memory stays bounded to
    /// the window; *time* is `O(document)` per window.
    FromStart,
}

/// A document's page boundaries, measured without its shaped galley ever
/// becoming resident.
///
/// This is what an open produces in the windowed path: the exact page count
/// from the first frame (`docs/113` §4 Q1), every page's geometry and model
/// span, and the checkpoints any page can be re-derived from.
#[derive(Clone, Debug)]
pub struct DocumentMeasures {
    /// Every page's boundaries, in order — identical to
    /// `paginate_document(...).pages` projected through
    /// [`PageOutline::of`](crate::measure::PageOutline::of).
    pub pages: Vec<PageOutline>,
    /// Resumable pagination checkpoints, every
    /// [`DEFAULT_CHECKPOINT_INTERVAL`] pages where the boundary allows one.
    pub checkpoints: Vec<Checkpoint>,
    /// The galley index each top-level flowed block starts at.
    block_starts: Vec<u32>,
    /// Total galley fragments (so the last block's extent is known).
    fragment_count: usize,
    /// The one section's page geometry.
    config: PageConfig,
    /// That section's resolved running content and page borders, so a window
    /// runs the same page-local post-passes the full driver runs.
    plan: SectionPlan,
    /// The width the body was flowed at.
    content_width: Twip,
    /// Where a window may start re-flowing.
    resume: FlowResume,
    /// The resolved note labels the measure pass flowed under, so a window
    /// re-flows under exactly the same ones.
    labels: NoteLabels,
    /// The endnotes the body references, in the order the driver appends
    /// their bodies to the flowed block sequence.
    ///
    /// Resolved once at open. Recomputing it per window would walk every block
    /// of the document on every scroll — which, on the file this work exists
    /// for, is a 1.3M-block walk to answer "no, there are no endnotes".
    endnotes: Vec<NoteId>,
    /// Top-level blocks in the flowed sequence, measured or not.
    total_blocks: usize,
    /// How many of them have **final** measures. Equal to `total_blocks` once
    /// the document is measured whole; below it while a prefix is still being
    /// extended (`docs/116` §7).
    committed_blocks: usize,
    /// The block the next chunk re-flows for context. One before
    /// `committed_blocks`, because the paragraph before a chunk's first is what
    /// `w:contextualSpacing` compares it against.
    resume_block: usize,
    /// The list counters as of entering `resume_block`.
    numbering: MeasureResume,
    /// The committed measures from `tail_base` onward — everything a resumed
    /// pagination can read.
    ///
    /// Not the whole measure list: an extension re-paginates from the last
    /// checkpoint, so nothing before it is ever read again, and retaining it
    /// would put the measure tier's fragments (not just its page outlines)
    /// resident for the whole document. The tail is bounded by one checkpoint
    /// interval of pages.
    tail: Vec<FragmentMeasure>,
    /// The galley index `tail[0]` sits at.
    tail_base: u32,
}

impl DocumentMeasures {
    /// The pages measured so far. Exact for the part of the document that has
    /// been measured, which is all of it once [`is_complete`](Self::is_complete)
    /// is true.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Whether every block has final measures, so [`page_count`](Self::page_count)
    /// is the document's page count and not a prefix's.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.committed_blocks >= self.total_blocks
    }

    /// How much of the document has final measures, as measured and total
    /// top-level blocks. What a host turns into a progress indicator.
    #[must_use]
    pub const fn progress(&self) -> (usize, usize) {
        (self.committed_blocks, self.total_blocks)
    }

    /// The document's page count when it is known, and an **estimate** scaled
    /// from the measured prefix while it is not.
    ///
    /// Never rounds down to the measured count while blocks remain: a reader
    /// told "42 pages" about a document still being measured would be told a
    /// number that is wrong in one direction only. A caller must present this
    /// as approximate whenever [`is_complete`](Self::is_complete) is false —
    /// `NUMPAGES`, the page indicator and print all read the same flag.
    #[must_use]
    pub fn estimated_page_count(&self) -> usize {
        if self.is_complete() || self.committed_blocks == 0 {
            return self.pages.len();
        }
        let scaled =
            (self.pages.len() as u128 * self.total_blocks as u128) / self.committed_blocks as u128;
        usize::try_from(scaled)
            .unwrap_or(usize::MAX)
            .max(self.pages.len() + 1)
    }

    /// The page geometry every page in this document uses.
    #[must_use]
    pub fn config(&self) -> &PageConfig {
        &self.config
    }

    /// Where a window may start re-flowing from.
    #[must_use]
    pub fn resume(&self) -> FlowResume {
        self.resume
    }

    /// The resident cost of the measure tier's own bookkeeping, in bytes —
    /// page outlines plus checkpoints plus the block map. Reported so a host
    /// can budget against a measured number rather than an estimate.
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        size_of_val(self.pages.as_slice())
            + size_of_val(self.block_starts.as_slice())
            + size_of_val(self.checkpoints.as_slice())
            + self
                .checkpoints
                .iter()
                .map(|c| c.table_headers.len() * size_of::<u32>())
                .sum::<usize>()
    }
}

/// Measures a document: every page boundary, with a bounded peak.
///
/// One shaping pass — the same pass an open already pays — through a
/// [`MeasureSink`](crate::measure::MeasureSink), so at most two shaped
/// paragraphs exist at a time however long the document is. Then one
/// [`paginate_measures`] over the heights.
///
/// # Errors
///
/// [`NotWindowable`] when a post-pagination pass could move a page boundary;
/// see the module docs for the table and for why each is refused rather than
/// approximated.
pub fn measure_document(
    document: &Document,
    shaper: &dyn LineShaper,
) -> Result<DocumentMeasures, NotWindowable> {
    measure_document_prefix(document, shaper, usize::MAX)
}

/// [`measure_document`] over the first `block_budget` top-level blocks only,
/// leaving the rest for [`extend_measures`].
///
/// `docs/116` §7. The measure pass is 88% of what opening a large document
/// costs and all of it runs before the first frame; a prefix is what makes the
/// first frame arrive in a bounded time regardless of how long the document is.
/// The pages it produces are not approximations of the real ones — pagination
/// is a forward fill and a windowable document has nothing that can move an
/// earlier boundary from later in the document — so the prefix's page
/// boundaries are final. Only the page *count* is unknown, and
/// [`DocumentMeasures::is_complete`] says so rather than letting a caller
/// present an estimate as exact.
///
/// # Errors
///
/// [`NotWindowable`], exactly as [`measure_document`].
pub fn measure_document_prefix(
    document: &Document,
    shaper: &dyn LineShaper,
    block_budget: usize,
) -> Result<DocumentMeasures, NotWindowable> {
    let sections = &document.definitions().sections;
    if sections.len() > 1 {
        return Err(NotWindowable::MultipleSections);
    }
    if sections.iter().any(|section| section.columns.count > 1) {
        return Err(NotWindowable::MultipleColumns);
    }
    if body_has_footnote_reference(document.body()) {
        return Err(NotWindowable::BodyFootnotes);
    }
    if document_has_anchored_object(document) {
        return Err(NotWindowable::AnchoredFloats);
    }
    if sections
        .iter()
        .any(|section| !section.line_numbering.is_empty())
    {
        return Err(NotWindowable::LineNumbering);
    }
    let labels = resolve_note_labels(document, None);
    if labels.restarts_each_page() {
        return Err(NotWindowable::NotesRestartEachPage);
    }

    // The section plan flows the headers/footers so the content area is the
    // one the body will actually be paginated into. Bounded by the running
    // content, not by the document.
    let plans = build_section_plans(document, shaper, &labels);
    let config = plans[0].config;
    let content_width = config.content_area().size.width;

    let endnotes = referenced_endnotes(document.body());
    let blocks = blocks_with_endnotes(document, document.body(), &endnotes);
    let mut measures = DocumentMeasures {
        pages: Vec::new(),
        checkpoints: Vec::new(),
        block_starts: Vec::new(),
        fragment_count: 0,
        config,
        plan: plans.into_iter().next().expect("one section, one plan"),
        content_width,
        resume: classify_resume(&blocks),
        labels,
        endnotes,
        total_blocks: blocks.len(),
        committed_blocks: 0,
        resume_block: 0,
        numbering: MeasureResume::default(),
        tail: Vec::new(),
        tail_base: 0,
    };
    measure_chunk(&mut measures, document, shaper, &blocks, block_budget);
    Ok(measures)
}

/// The smallest chunk that can commit anything. `block_budget` counts blocks
/// **committed**, not blocks flowed, so the floor is one.
const MIN_CHUNK_BLOCKS: usize = 1;

/// Measures the next `block_budget` blocks of a document whose measures are a
/// prefix, and re-paginates from the last checkpoint.
///
/// Returns whether the document is now measured whole. Cheap to call on one
/// that already is: it does nothing and answers `true`.
///
/// # Panics
///
/// If `document` is not the document `measures` was produced from. The blocks
/// are re-derived from it, and measuring a prefix of one document into the
/// measures of another would produce a page list belonging to neither.
pub fn extend_measures(
    measures: &mut DocumentMeasures,
    document: &Document,
    shaper: &dyn LineShaper,
    block_budget: usize,
) -> bool {
    if measures.is_complete() {
        return true;
    }
    let blocks = blocks_with_endnotes(document, document.body(), &measures.endnotes);
    assert_eq!(
        blocks.len(),
        measures.total_blocks,
        "extend_measures was given a different document from the one measured",
    );
    measure_chunk(measures, document, shaper, &blocks, block_budget);
    measures.is_complete()
}

/// Flows one chunk and commits the blocks whose measures it settles.
///
/// The chunk is `blocks[resume_block ..= end]`, and it commits
/// `blocks[resume_block + 1 .. end]` — one block of overlap at each end,
/// discarded:
///
/// - **the first**, because `w:contextualSpacing` suppresses the gap between
///   two adjacent paragraphs of the same style, so the chunk's first committed
///   block needs the one before it present to compare against. It is already
///   committed; this pass only re-establishes the context;
/// - **the last**, because two things reach one block *forward* — the
///   space-after half of that same collapse, and the drop-cap pair, which only
///   exists when a paragraph follows the head. Its measures here would be
///   provisional, so the next chunk commits it instead.
///
/// The counters cross the seam through [`MeasureResume`]; nothing else the flow
/// carries survives a top-level block boundary.
fn measure_chunk(
    measures: &mut DocumentMeasures,
    document: &Document,
    shaper: &dyn LineShaper,
    blocks: &[BlockNode],
    block_budget: usize,
) {
    let from = measures.resume_block;
    let budget = block_budget.max(MIN_CHUNK_BLOCKS);
    // `block_budget` is how many blocks to COMMIT; the flowed slice is that
    // plus the trailing context block, and it starts at `resume_block` rather
    // than at the commit boundary so the leading context block is in it too.
    let to = measures
        .committed_blocks
        .saturating_add(budget)
        .saturating_add(1)
        .min(blocks.len());
    if to <= from {
        return;
    }
    let (chunk, starts, next_resume) = build_measures_for_blocks_resumed(
        document,
        shaper,
        &blocks[from..to],
        measures.content_width,
        None,
        ReviewView::Editing,
        NoteFlow::with_labels(&measures.labels),
        single_section_line_grid(document),
        measures.numbering.clone(),
    );

    // Local block indices this chunk commits. The leading block is context
    // rather than content exactly when it has already been committed — which
    // is every chunk but the first, including the second, whose `resume_block`
    // is 0 and whose leading block is therefore block 0.
    let leading = usize::from(from < measures.committed_blocks);
    let reaches_end = to == blocks.len();
    let local_end = (to - from) - usize::from(!reaches_end);
    if local_end <= leading {
        // Nothing to commit: the chunk is one block of context and no body.
        // Only reachable at the very end of a document, where `reaches_end`
        // makes it impossible, so this is a guard against a future budget of 1.
        return;
    }

    let first_fragment = starts.get(leading).copied().unwrap_or(0) as usize;
    let last_fragment = starts
        .get(local_end)
        .map_or(chunk.len(), |index| *index as usize);
    let base = measures.fragment_count as u32;
    for local in leading..local_end {
        let start = starts.get(local).copied().unwrap_or(0) as usize;
        measures
            .block_starts
            .push(base + (start - first_fragment) as u32);
    }
    let committed = &chunk[first_fragment..last_fragment];
    measures.tail.extend_from_slice(committed);
    measures.fragment_count += committed.len();
    measures.committed_blocks = from + local_end;
    measures.resume_block = measures.committed_blocks.saturating_sub(1);
    measures.numbering = next_resume;

    repaginate_tail(measures);
}

/// Re-paginates from the last checkpoint over the retained tail, and trims the
/// tail back to what the new last checkpoint needs.
///
/// Resuming rather than re-running is what keeps extending a long document
/// linear in its length: a whole `paginate_measures` per chunk would be
/// quadratic, which is the shape `docs/116` exists to stop introducing.
fn repaginate_tail(measures: &mut DocumentMeasures) {
    let resume_from = measures
        .checkpoints
        .iter()
        .rev()
        .find(|checkpoint| {
            checkpoint.at.fragment >= measures.tail_base
                && checkpoint
                    .table_headers
                    .iter()
                    .all(|index| *index >= measures.tail_base)
        })
        .cloned();

    let layout = match &resume_from {
        Some(checkpoint) => {
            measures.pages.truncate(checkpoint.page_index as usize);
            measures
                .checkpoints
                .retain(|recorded| recorded.page_index <= checkpoint.page_index);
            paginate_measures_from(
                &measures.tail,
                measures.tail_base,
                &measures.config,
                DEFAULT_CHECKPOINT_INTERVAL,
                checkpoint,
            )
        }
        None => {
            // No checkpoint the tail covers, so the tail is the whole galley —
            // a document below one checkpoint interval, or one whose boundaries
            // are never resumable. Both re-paginate whole, over a tail that is
            // correspondingly short.
            debug_assert_eq!(
                measures.tail_base, 0,
                "a tail with no checkpoint is the galley"
            );
            measures.pages.clear();
            measures.checkpoints.clear();
            paginate_measures(
                &measures.tail,
                &measures.config,
                DEFAULT_CHECKPOINT_INTERVAL,
            )
        }
    };
    measures.pages.extend(layout.pages);
    measures.checkpoints.extend(layout.checkpoints);

    // Everything before the last checkpoint is unreachable from here on.
    if measures.is_complete() {
        measures.tail = Vec::new();
        measures.tail_base = measures.fragment_count as u32;
        return;
    }
    let Some(last) = measures.checkpoints.last() else {
        return;
    };
    let keep_from = last
        .table_headers
        .iter()
        .copied()
        .chain(core::iter::once(last.at.fragment))
        .min()
        .unwrap_or(last.at.fragment);
    if keep_from > measures.tail_base {
        measures
            .tail
            .drain(..(keep_from - measures.tail_base) as usize);
        measures.tail_base = keep_from;
    }
}

/// Whether any block references a **footnote**. Endnotes are fine — their
/// bodies are appended to the flowed block sequence by [`windowed_blocks`],
/// exactly as the full driver appends them — but a footnote reserves a band at
/// the bottom of the page it lands on, which changes the height pagination
/// fills.
fn body_has_footnote_reference(blocks: &[BlockNode]) -> bool {
    let mut found = false;
    for block in blocks {
        visit_block_note_refs(block, &mut |kind, _| {
            if kind == NoteKind::Footnote {
                found = true;
            }
        });
    }
    found
}

/// Whether anything in `blocks` carries flow state across a top-level block
/// boundary. See [`FlowResume`].
///
/// Conservative in the safe direction: any doubt answers [`FlowResume::FromStart`].
fn classify_resume(blocks: &[BlockNode]) -> FlowResume {
    let carries_state = blocks.iter().any(|block| match block {
        BlockNode::Paragraph(paragraph) => {
            paragraph.properties.numbering.is_some()
                || paragraph.properties.contextual_spacing
                || paragraph.properties.drop_cap_frame.is_some()
                || paragraph.properties.style_ref.is_some()
        }
        // A table, a content control or an external chunk is not analyzed;
        // take the safe answer.
        _ => true,
    });
    if carries_state {
        FlowResume::FromStart
    } else {
        FlowResume::AnyBlock
    }
}

/// How much paint-tier memory a window may hold, and how far either side of
/// the visible pages it reaches.
///
/// `docs/113` §4 Q3: the budget is in **bytes, not pages**, because a page of
/// prose and a page of dense table differ by orders of magnitude and a
/// page-counted budget is therefore a budget for the wrong thing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowPolicy {
    /// Paint-tier byte budget. A window that would exceed it is trimmed toward
    /// the visible pages rather than refused — the visible pages are always
    /// built, whatever they cost, because a page the user is looking at that
    /// cannot be painted is not a memory saving.
    pub budget_bytes: usize,
    /// Pages to build either side of the visible range — "one screenful".
    pub lead_pages: usize,
}

/// The starting budget `docs/113` §4 Q3 names: 256 MiB of paint tier.
pub const DEFAULT_PAINT_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// Pages built either side of the visible range by default.
pub const DEFAULT_LEAD_PAGES: usize = 2;

impl Default for WindowPolicy {
    fn default() -> Self {
        Self {
            budget_bytes: DEFAULT_PAINT_BUDGET_BYTES,
            lead_pages: DEFAULT_LEAD_PAGES,
        }
    }
}

impl WindowPolicy {
    /// The page range to build for `visible`, widened by
    /// [`lead_pages`](Self::lead_pages) and clamped to `total_pages`.
    #[must_use]
    pub fn window_for(&self, visible: PageRange, total_pages: usize) -> PageRange {
        let start = visible.start.saturating_sub(self.lead_pages);
        let end = visible.end.saturating_add(self.lead_pages).min(total_pages);
        PageRange::new(start.min(end), end)
    }
}

/// One materialized window: the pages, and what the paint tier cost.
#[derive(Clone, Debug)]
pub struct PaintWindow {
    /// The pages, with absolute numbers and stacked y-offsets, plus the true
    /// total page count.
    pub viewport: ViewportLayout,
    /// The page range actually built (after any budget trim).
    pub range: PageRange,
    /// Paint-tier bytes the built pages hold, by [`page_paint_bytes`].
    pub bytes: usize,
    /// Galley fragments re-shaped to build this window — the work it cost, so
    /// a thrash guard can assert it stayed bounded.
    pub shaped_fragments: usize,
    /// The page index the paginator resumed at: a recorded checkpoint's, or
    /// `0` for the implicit one at the document start. Reported so a test can
    /// tell "the window happened to start at page 0" from "the window resumed
    /// from a real checkpoint".
    pub resumed_at_page: usize,
}

/// Builds the paint tier for one window of pages.
///
/// Each returned page is field-for-field the page a full
/// [`paginate_document`](crate::document_layout::paginate_document) would
/// produce, because it comes from the same paginator resumed at a checkpoint
/// the measure pass recorded, over fragments the same flow engine produced.
///
/// The budget trims the window toward `visible`; the visible pages themselves
/// are always built.
#[must_use]
pub fn window_of(
    document: &Document,
    shaper: &dyn LineShaper,
    measures: &DocumentMeasures,
    visible: PageRange,
    policy: WindowPolicy,
) -> PaintWindow {
    let total = measures.pages.len();
    let visible = PageRange::new(visible.start.min(total), visible.end.min(total));
    let wanted = policy.window_for(visible, total);
    if wanted.start >= wanted.end {
        return PaintWindow {
            viewport: ViewportLayout {
                total_pages: total,
                pages: Vec::new(),
            },
            range: wanted,
            bytes: 0,
            shaped_fragments: 0,
            resumed_at_page: 0,
        };
    }

    let checkpoint = checkpoint_at_or_before(measures, wanted.start);
    // Everything from the resume point to the end of the last wanted page.
    let need_from = checkpoint
        .table_headers
        .iter()
        .copied()
        .fold(checkpoint.at.fragment, u32::min) as usize;
    let need_to = (measures.pages[wanted.end - 1].flow.end.fragment as usize + 1)
        .min(measures.fragment_count);

    let blocks = blocks_with_endnotes(document, document.body(), &measures.endnotes);
    let from_block = match measures.resume {
        FlowResume::AnyBlock => measures
            .block_starts
            .partition_point(|start| (*start as usize) <= need_from)
            .saturating_sub(1),
        FlowResume::FromStart => 0,
    };
    let (galley, base) = flow_body_range(
        document,
        shaper,
        &blocks,
        measures.content_width,
        from_block,
        need_from..need_to,
        &measures.block_starts,
        NoteFlow::with_labels(&measures.labels),
        single_section_line_grid(document),
    );
    let shaped_fragments = galley.len();

    let paginated = paginate_from_based(&galley, base, &measures.config, checkpoint);
    // `paginate_from` runs to the end of the fragments it is given, which is
    // the end of the window plus whatever tail the last page swept in. Keep
    // only the wanted pages.
    let first = checkpoint.page_index as usize;
    let mut pages: Vec<Page> = paginated.pages;
    let take_from = wanted.start.saturating_sub(first);
    let take_to = (wanted.end - first).min(pages.len());
    let pages = if take_from < take_to {
        pages.drain(take_from..take_to).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let pages = finish_window_pages(document, shaper, measures, wanted.start, pages);

    // Budget trim (`docs/113` §4 Q3). The visible pages are charged first and
    // are never dropped — a page the user is looking at that cannot be painted
    // is not a memory saving, it is a blank screen. The lead pages are then
    // added outward from the visible range, ahead first (scrolling down is the
    // common direction), while the budget allows, and the result stays a
    // contiguous range because that is what a scroll window is.
    let costs: Vec<usize> = pages.iter().map(page_paint_bytes).collect();
    let visible_lo = visible.start.max(wanted.start);
    let visible_hi = visible.end.min(wanted.end).max(visible_lo);
    let mut lo = visible_lo - wanted.start;
    let mut hi = (visible_hi - wanted.start).max(lo);
    let mut bytes: usize = costs[lo..hi].iter().sum();
    loop {
        let grew_after = hi < pages.len() && bytes + costs[hi] <= policy.budget_bytes;
        if grew_after {
            bytes += costs[hi];
            hi += 1;
        }
        let grew_before = lo > 0 && bytes + costs[lo - 1] <= policy.budget_bytes;
        if grew_before {
            lo -= 1;
            bytes += costs[lo];
        }
        if !grew_after && !grew_before {
            break;
        }
    }

    let page_height = i64::from(measures.config.page_size.height.raw());
    let first_index = wanted.start + lo;
    let visible_pages: Vec<VisiblePage> = pages
        .into_iter()
        .enumerate()
        .filter(|(offset, _)| *offset >= lo && *offset < hi)
        .map(|(offset, page)| VisiblePage {
            y_offset: stacked_offset(page_height, wanted.start + offset),
            page,
        })
        .collect();
    let built = PageRange::new(first_index, first_index + visible_pages.len());

    PaintWindow {
        viewport: ViewportLayout {
            total_pages: total,
            pages: visible_pages,
        },
        range: built,
        bytes,
        shaped_fragments,
        resumed_at_page: checkpoint.page_index as usize,
    }
}

/// Runs the driver's **page-local** post-passes over one window's pages, so a
/// windowed page is a finished page and not a half-built one.
///
/// `docs/113` §7's fourth unknown is that `paginate_from` reproduces the
/// *paginator's* output, not the driver's. Four of the driver's passes are
/// page-local given the page's absolute number, and run here:
///
/// - section `w:vAlign`, which shifts a page's placed content within its own
///   content area;
/// - running content, selected by this page's number within its section;
/// - `w:pgBorders`, resolved against that same number;
/// - `PAGE`/`NUMPAGES` field resolution — and `NUMPAGES` is exactly the number
///   the measure tier supplies, which is why a window can resolve it at all.
///
/// The passes that are **not** page-local — anchored floats (a document-global
/// z-order and a body-wide wrap fixed point) and margin line numbers (a
/// running counter across pages) — are not run here and cannot be: a document
/// carrying either is refused by [`measure_document`] instead.
fn finish_window_pages(
    document: &Document,
    shaper: &dyn LineShaper,
    measures: &DocumentMeasures,
    first_index: usize,
    pages: Vec<Page>,
) -> Vec<Page> {
    if pages.is_empty() {
        return pages;
    }
    let sections = &document.definitions().sections;
    let section = sections.first();
    let plan = &measures.plan;
    let mirror = document.definitions().settings.mirror_margins;
    let mut layout = crate::page::PaginatedLayout { pages };
    apply_page_vertical_alignment(&mut layout, sections);
    for page in &mut layout.pages {
        // One section, so a page's number within its section is its number.
        let section_page_number = page.number;
        let config = mirrored_page_config(&plan.config, mirror, page.number);
        place_running_content_on_page(page, &plan.running, &config, section_page_number);
        page.page_borders = resolve_page_borders(
            &plan.page_borders,
            section_page_number,
            page.page_size,
            page.content_area,
        );
    }
    let labels: Vec<String> = (0..layout.pages.len())
        .map(|offset| page_number_label_at(section, first_index + offset))
        .collect();
    resolve_fields_labeled_with_total(&mut layout, &labels, measures.pages.len() as u32, shaper);
    layout.pages
}

/// The last checkpoint at or before page `index`, or the implicit one at the
/// document start.
fn checkpoint_at_or_before(measures: &DocumentMeasures, index: usize) -> &Checkpoint {
    static START: std::sync::OnceLock<Checkpoint> = std::sync::OnceLock::new();
    let slot = measures
        .checkpoints
        .partition_point(|c| (c.page_index as usize) <= index);
    match slot
        .checked_sub(1)
        .and_then(|i| measures.checkpoints.get(i))
    {
        Some(checkpoint) => checkpoint,
        None => START.get_or_init(|| Checkpoint {
            page_index: 0,
            at: crate::page::FlowPos::at(0),
            current_table: None,
            table_headers: Vec::new(),
        }),
    }
}

/// The top of page `index` in a continuous scroll, saturating rather than
/// overflowing a twip for a pathologically long document.
fn stacked_offset(page_height: i64, index: usize) -> Twip {
    let raw = page_height.saturating_mul(index as i64);
    Twip(raw.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

/// The paint-tier bytes one page holds: the glyph-bearing structures the
/// window budget exists to bound.
///
/// Counted, not estimated, by
/// [`BlockFragment::paint_bytes`](crate::block::BlockFragment::paint_bytes).
#[must_use]
pub fn page_paint_bytes(page: &Page) -> usize {
    let placed = |list: &[crate::page::PlacedFragment]| -> usize {
        size_of_val(list) + list.iter().map(|p| p.fragment.paint_bytes()).sum::<usize>()
    };
    size_of::<Page>() + placed(&page.placed) + placed(&page.header) + placed(&page.footer)
}

// --- Scroll coalescing ------------------------------------------------------

/// What a scroll position asks the host to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollDecision {
    /// Build the window for this visible range now.
    Build(PageRange),
    /// The position has not settled; build nothing yet.
    Wait,
    /// The window already built covers this position.
    Satisfied,
}

/// Coalesces scroll positions so a drag builds the window the user **lands
/// on**, not every window they pass over.
///
/// `docs/113` §4 Q3: "dragging a scrollbar across 60,000 pages must not
/// thrash". Without coalescing, a drag that reports a hundred intermediate
/// positions asks for a hundred windows, each of which re-flows and re-shapes
/// — the user sees a stall and the machine does a hundred times the necessary
/// work for content nobody looked at.
///
/// Deliberately **clock-free**. The engine has no wall clock it can rely on
/// across native, wasm and headless hosts, and a test that waits on a clock is
/// the kind that "degrades under load no matter how sound the code is". The
/// host drives [`tick`](Self::tick) from whatever it already has — a frame
/// callback, a debounce timer — and the settle threshold is counted in ticks.
///
/// The first window is built **immediately**: there is nothing on screen yet,
/// and making the first paint wait for a settle is a blank page, not a saving.
#[derive(Clone, Debug)]
pub struct ScrollCoalescer {
    /// Ticks a position must hold still before its window is built.
    settle_ticks: u32,
    /// The position waiting to settle.
    pending: Option<PageRange>,
    /// Consecutive ticks `pending` has not moved.
    still: u32,
    /// The window the host has actually built, as it reported it.
    built: Option<PageRange>,
    /// How many builds this coalescer has asked for — the thrash counter.
    builds: usize,
}

impl ScrollCoalescer {
    /// A coalescer that builds the window at a new position on the
    /// `settle_ticks`-th host tick after the position last moved. `0` and `1`
    /// both build on the very next tick.
    #[must_use]
    pub fn new(settle_ticks: u32) -> Self {
        Self {
            settle_ticks,
            pending: None,
            still: 0,
            built: None,
            builds: 0,
        }
    }

    /// The window the host last reported building.
    #[must_use]
    pub fn built(&self) -> Option<PageRange> {
        self.built
    }

    /// How many windows this coalescer has asked the host to build.
    #[must_use]
    pub fn builds(&self) -> usize {
        self.builds
    }

    /// The host scrolled so that `visible` is on screen.
    pub fn scrolled_to(&mut self, visible: PageRange) -> ScrollDecision {
        if self.covers(visible) {
            self.pending = None;
            self.still = 0;
            return ScrollDecision::Satisfied;
        }
        if self.built.is_none() {
            self.pending = None;
            self.still = 0;
            self.builds += 1;
            return ScrollDecision::Build(visible);
        }
        if self.pending != Some(visible) {
            self.still = 0;
        }
        self.pending = Some(visible);
        ScrollDecision::Wait
    }

    /// One settle tick from the host.
    pub fn tick(&mut self) -> ScrollDecision {
        let Some(pending) = self.pending else {
            return ScrollDecision::Satisfied;
        };
        self.still = self.still.saturating_add(1);
        if self.still < self.settle_ticks {
            return ScrollDecision::Wait;
        }
        self.pending = None;
        self.still = 0;
        self.builds += 1;
        ScrollDecision::Build(pending)
    }

    /// Records the range the host actually built, which is what later
    /// positions are tested against — the built range is wider than the
    /// visible one (the lead pages), and narrower when the byte budget
    /// trimmed it, so only the host knows it.
    pub fn record_built(&mut self, range: PageRange) {
        self.built = Some(range);
    }

    /// Whether the built window covers `visible` entirely.
    fn covers(&self, visible: PageRange) -> bool {
        match self.built {
            Some(built) => visible.start >= built.start && visible.end <= built.end,
            None => false,
        }
    }
}
