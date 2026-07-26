//! Incremental wiring — the bridge that turns `repaginate` into an editor loop.
//!
//! The paginator ([`crate::paginate`]) already knows how to re-flow only the
//! neighborhood of an edit (`repaginate`, the stabilization halt, `43-…` §3.4,
//! §7.4). This module wires that machinery to what an editor actually has after a
//! model transaction — a set of changed node ids — and adds the two pieces that
//! make the incremental engine usable:
//!
//! 1. **Dirty tracking** ([`DirtySet`], [`reflow`]): map the model's changed
//!    [`casual_doc_model::NodeId`]s to the affected galley fragment and re-flow.
//! 2. **A paragraph-level galley cache** ([`GalleyCache`], paired with
//!    [`crate::flow::build_galley_cached`]) so rebuilding the galley after an edit
//!    re-shapes only the dirty paragraphs — the expensive step — reusing the
//!    shaped lines of every paragraph that did not change. This is what makes an
//!    edit `O(edit)` rather than `O(document)`.
//! 3. **A virtualized viewport** ([`paginate_viewport`], [`viewport_of`]): expose
//!    only the pages intersecting the visible scroll window — with their absolute
//!    page number and stacked y-offset — while still reporting the true total page
//!    count, so a host only composes and paints what is on screen.
//!
//! The layout produced here is, by construction, field-for-field identical to a
//! full [`crate::paginate::paginate`]: dirty tracking chooses *where* to resume,
//! never *what* the answer is (`repaginate` owns the golden invariant), and the
//! viewport is a windowed view of that same layout.

use std::collections::{BTreeSet, HashMap};

use casual_doc_model::NodeId;

use crate::block::BlockFragment;
use crate::page::{Page, PaginatedLayout};
use crate::paginate::{PageConfig, RepaginateStats, paginate, repaginate_with_stats};
use crate::units::Twip;

// --- Dirty tracking --------------------------------------------------------

/// The set of model nodes a transaction reported as changed — the *damage set*
/// at block granularity (`43-…` §7.4 step 1). An editor collects the node ids a
/// `casual-doc-model` edit touched (a run rewritten, a paragraph split/joined,
/// properties changed) and hands them here; the incremental engine uses them to
/// invalidate exactly the affected galley fragments and cached paragraph lines.
///
/// A [`DirtySet::everything`] set invalidates the whole document (the safe
/// fallback when the damage cannot be scoped — e.g. a section-geometry change).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirtySet {
    /// When set, every node is considered dirty regardless of `nodes`.
    all: bool,
    /// The explicitly changed nodes.
    nodes: BTreeSet<NodeId>,
}

impl DirtySet {
    /// An empty damage set (nothing changed).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A damage set that invalidates every node — the whole-document fallback.
    #[must_use]
    pub fn everything() -> Self {
        Self {
            all: true,
            nodes: BTreeSet::new(),
        }
    }

    /// Marks node `id` as changed.
    pub fn mark(&mut self, id: NodeId) -> &mut Self {
        self.nodes.insert(id);
        self
    }

    /// Whether this set invalidates every node.
    #[must_use]
    pub fn is_all(&self) -> bool {
        self.all
    }

    /// Whether node `id` is dirty (always true for an `everything` set).
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.all || self.nodes.contains(&id)
    }

    /// Whether nothing is invalidated (neither `all` nor any explicit node).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.all && self.nodes.is_empty()
    }

    /// The number of explicitly marked nodes (an `everything` set reports the
    /// count of nodes still listed explicitly, which may be zero).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Iterates the explicitly marked nodes.
    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().copied()
    }
}

impl FromIterator<NodeId> for DirtySet {
    fn from_iter<I: IntoIterator<Item = NodeId>>(iter: I) -> Self {
        Self {
            all: false,
            nodes: iter.into_iter().collect(),
        }
    }
}

impl Extend<NodeId> for DirtySet {
    fn extend<I: IntoIterator<Item = NodeId>>(&mut self, iter: I) {
        self.nodes.extend(iter);
    }
}

/// The index of the first galley fragment affected by `dirty`, or `None` if no
/// fragment in `galley` carries a dirty node. This is the model-to-galley bridge:
/// a `casual-doc-model` transaction reports node ids, and each `BlockFragment`
/// carries the node it came from (`BlockFragment::node_id`), so the earliest
/// changed fragment is the earliest fragment whose node is dirty. An `everything`
/// set maps to the first fragment.
#[must_use]
pub fn first_dirty_fragment(galley: &[BlockFragment], dirty: &DirtySet) -> Option<usize> {
    if dirty.is_all() {
        return (!galley.is_empty()).then_some(0);
    }
    galley.iter().position(|f| dirty.contains(f.node_id()))
}

/// Re-paginates `new_galley` after a model transaction reported `changed_nodes`,
/// reusing the previous layout everywhere the edit did not disturb it. This is
/// the clean editor entry point: given the previous layout, the galley it came
/// from, the freshly (cache-)rebuilt galley, and the transaction's damage set, it
/// returns a layout **field-for-field identical to a full
/// [`crate::paginate::paginate`]** of `new_galley` — the golden invariant, which
/// [`repaginate_with_stats`] owns.
///
/// `changed_nodes` is the model-side damage; the galley-content diff inside
/// `repaginate` is the layout-side source of truth. The two must agree (the
/// galley can only differ at or after the first fragment whose node was reported
/// dirty), which is asserted in debug builds — a caller that forgets to report a
/// touched node is a bug worth catching early.
#[must_use]
pub fn reflow(
    prev: &PaginatedLayout,
    prev_galley: &[BlockFragment],
    new_galley: &[BlockFragment],
    changed_nodes: &DirtySet,
    config: &PageConfig,
) -> PaginatedLayout {
    reflow_with_stats(prev, prev_galley, new_galley, changed_nodes, config).0
}

/// [`reflow`] plus the [`RepaginateStats`] describing how much work the edit
/// actually cost (reused-prefix / reflowed / reused-tail).
#[must_use]
pub fn reflow_with_stats(
    prev: &PaginatedLayout,
    prev_galley: &[BlockFragment],
    new_galley: &[BlockFragment],
    changed_nodes: &DirtySet,
    config: &PageConfig,
) -> (PaginatedLayout, RepaginateStats) {
    debug_assert!(
        damage_covers_galley_diff(prev_galley, new_galley, changed_nodes),
        "the reported damage set must cover the first galley fragment that changed"
    );
    repaginate_with_stats(prev, prev_galley, new_galley, config)
}

/// Whether `dirty` accounts for the first place the two galleys diverge — the
/// consistency check behind [`reflow_with_stats`]'s debug assertion. The first
/// fragment that differs (or the first extra fragment when one galley is longer)
/// must carry a node the transaction reported dirty.
fn damage_covers_galley_diff(
    prev: &[BlockFragment],
    new: &[BlockFragment],
    dirty: &DirtySet,
) -> bool {
    if dirty.is_all() {
        return true;
    }
    let common = prev.len().min(new.len());
    for i in 0..common {
        if prev[i] != new[i] {
            return dirty.contains(new[i].node_id()) || dirty.contains(prev[i].node_id());
        }
    }
    if let Some(extra) = new.get(common) {
        return dirty.contains(extra.node_id());
    }
    if let Some(extra) = prev.get(common) {
        return dirty.contains(extra.node_id());
    }
    true
}

// --- Paragraph-level galley cache ------------------------------------------

/// A shaped paragraph kept for reuse across edits: the content hash it was shaped
/// under and the resulting fragment.
#[derive(Clone, Debug)]
struct CachedParagraph {
    /// Hash of every input that determines the shaped fragment (run text and
    /// properties, wrap width, alignment, box metrics, break control).
    hash: u64,
    /// The shaped paragraph fragment, cloned into each rebuilt galley on a hit.
    fragment: BlockFragment,
}

/// A cache of shaped paragraph fragments, keyed by paragraph
/// [`casual_doc_model::NodeId`], that lets [`crate::flow::build_galley_cached`]
/// rebuild the galley after an edit while re-shaping only the paragraphs that
/// actually changed.
///
/// Shaping (bidi, line breaking, glyph positioning — the `LineShaper`) is by far
/// the most expensive step of building a galley; paginating the resulting
/// fragments is cheap by comparison. So caching the shaped lines of unchanged
/// paragraphs is what turns a keystroke from `O(document)` into `O(edit)`.
///
/// A fragment is reused only when its cached content hash still matches *and* its
/// node was not reported dirty, so both a stale caller damage set and a content
/// change independently force a re-shape. The cache is scoped to a single wrap
/// width; a width change (a resize, a margin edit) clears it, since every
/// paragraph must re-wrap.
#[derive(Clone, Debug, Default)]
pub struct GalleyCache {
    /// The wrap width all cached entries were shaped at (`None` until first use).
    width: Option<Twip>,
    /// Shaped fragments by paragraph node.
    entries: HashMap<NodeId, CachedParagraph>,
    /// Paragraphs (re-)shaped during the most recent build — telemetry that lets
    /// tests and benchmarks confirm work stayed proportional to the edit.
    shaped_last_build: usize,
}

impl GalleyCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of cached paragraphs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no paragraphs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many paragraphs were (re-)shaped during the most recent
    /// [`crate::flow::build_galley_cached`] — the cache misses, i.e. the work the
    /// last rebuild actually cost.
    #[must_use]
    pub fn shaped_last_build(&self) -> usize {
        self.shaped_last_build
    }

    /// Begins a rebuild at `width`: resets the per-build shaped counter and, if
    /// the wrap width changed, clears every entry (all paragraphs must re-wrap).
    pub(crate) fn begin_build(&mut self, width: Twip) {
        if self.width != Some(width) {
            self.entries.clear();
            self.width = Some(width);
        }
        self.shaped_last_build = 0;
    }

    /// The cached fragment reusable for node `id` under content `hash`: a hit
    /// requires a matching hash and a node that was not reported dirty.
    pub(crate) fn reusable(
        &self,
        id: NodeId,
        hash: u64,
        dirty: &DirtySet,
    ) -> Option<&BlockFragment> {
        if dirty.contains(id) {
            return None;
        }
        self.entries
            .get(&id)
            .filter(|entry| entry.hash == hash)
            .map(|entry| &entry.fragment)
    }

    /// Records a freshly shaped `fragment` for node `id` under content `hash` and
    /// counts the re-shape against the current build.
    pub(crate) fn store(&mut self, id: NodeId, hash: u64, fragment: BlockFragment) {
        self.shaped_last_build += 1;
        self.entries.insert(id, CachedParagraph { hash, fragment });
    }
}

// --- Virtualized viewport --------------------------------------------------

/// A half-open range of 0-based page indices `[start, end)` — the pages a scroll
/// window intersects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRange {
    /// First page index in the window (0-based, inclusive).
    pub start: usize,
    /// One past the last page index in the window (0-based, exclusive).
    pub end: usize,
}

impl PageRange {
    /// A range `[start, end)`.
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The single-page window `[index, index + 1)`.
    #[must_use]
    pub fn single(index: usize) -> Self {
        Self {
            start: index,
            end: index + 1,
        }
    }
}

/// One page inside the visible window: its stacked y-offset in the continuous
/// scroll (twips from the document top) and the page itself. The page carries its
/// own absolute 1-based number (`Page::number`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisiblePage {
    /// Top of this page in the continuous vertical scroll, in twips from the
    /// document top (page index times the full page height).
    pub y_offset: Twip,
    /// The laid-out page — identical to the same page of a full paginate.
    pub page: Page,
}

/// A windowed view of a paginated layout: only the pages a scroll window
/// intersects, plus the true total page count so a scrollbar and page counter are
/// correct without materializing the off-screen pages downstream.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ViewportLayout {
    /// Total pages in the whole document (not just the window).
    pub total_pages: usize,
    /// The visible pages, in order, each with its absolute y-offset.
    pub pages: Vec<VisiblePage>,
}

/// Windows an already-computed [`PaginatedLayout`] to the pages `range` covers —
/// the cheap scroll path. In an editor the full layout is computed once (and kept
/// fresh incrementally by [`reflow`]); every subsequent scroll is just this
/// windowing, which touches only the visible pages, so composing and painting
/// stays bounded to the viewport (`43-…` §7.4 step 4).
///
/// Each returned page equals the corresponding page of the source layout
/// field-for-field, tagged with its absolute stacked y-offset. `range` is clamped
/// to the available pages.
#[must_use]
pub fn viewport_of(
    layout: &PaginatedLayout,
    config: &PageConfig,
    range: PageRange,
) -> ViewportLayout {
    let total_pages = layout.pages.len();
    let end = range.end.min(total_pages);
    let start = range.start.min(end);
    let page_height = i64::from(config.page_size.height.raw());
    let pages = (start..end)
        .map(|i| VisiblePage {
            y_offset: stacked_offset(page_height, i),
            page: layout.pages[i].clone(),
        })
        .collect();
    ViewportLayout { total_pages, pages }
}

/// Paginates `fragments` and returns only the pages intersecting `range`, with
/// the true total page count and each visible page's absolute number and stacked
/// y-offset — the virtualized viewport (`43-…` §7.4).
///
/// The result agrees field-for-field with the corresponding slice of a full
/// [`crate::paginate::paginate`]. Pagination over a galley is `O(fragments)` and
/// cheap (the expensive shaping is virtualized separately by [`GalleyCache`]);
/// the viewport bounds the *downstream* cost — composition and paint run only for
/// the returned pages. For repeated scrolling over a stable document, hold the
/// layout and window it with [`viewport_of`] instead of re-paginating.
#[must_use]
pub fn paginate_viewport(
    fragments: &[BlockFragment],
    config: &PageConfig,
    range: PageRange,
) -> ViewportLayout {
    let layout = paginate(fragments, config);
    viewport_of(&layout, config, range)
}

/// The top of page `index` in a continuous scroll where pages stack by their full
/// height, saturating rather than overflowing a twip for a pathologically long
/// document.
fn stacked_offset(page_height: i64, index: usize) -> Twip {
    let raw = page_height.saturating_mul(index as i64);
    Twip(raw.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use casual_doc_model::v1::{
        BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
        RunProperties,
    };

    use super::*;
    use crate::flow::{build_galley, build_galley_cached};
    use crate::model::ModelRange;
    use crate::text::{Line, LineBreak, LineConstraints, LineLayout, LineShaper, StyledRun};
    use crate::units::Size;
    use casual_doc_model::v1::SectionId;

    /// A `LineShaper` that counts its calls (to prove the cache re-shapes only
    /// dirty paragraphs) and encodes the run text length into the line height (so
    /// a content change produces an observably different fragment).
    struct SpyShaper {
        calls: Cell<usize>,
    }

    impl SpyShaper {
        fn new() -> Self {
            Self {
                calls: Cell::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.get()
        }
    }

    impl LineShaper for SpyShaper {
        fn shape_paragraph(
            &self,
            runs: &[StyledRun<'_>],
            _constraints: LineConstraints,
            range: ModelRange,
        ) -> LineLayout {
            self.calls.set(self.calls.get() + 1);
            let text_len: i32 = runs.iter().map(|r| r.text.len() as i32).sum();
            let height = Twip(240 + text_len * 10);
            LineLayout {
                lines: vec![Line {
                    runs: Vec::new(),
                    ascent: height,
                    descent: Twip::ZERO,
                    height,
                    range,
                    line_break: LineBreak::ParagraphEnd,
                    page_break_after: false,
                    bars: Vec::new(),
                    images: Vec::new(),
                    fields: Vec::new(),
                    text_boxes: Vec::new(),
                    rules: Vec::new(),
                }],
            }
        }
    }

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    /// A document whose i-th paragraph (node `i + 1`) contains `texts[i]` as a
    /// single run (node `i + 1_000`).
    fn doc(texts: &[&str]) -> Document {
        let body = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let id = i as u64 + 1;
                BlockNode::Paragraph(Paragraph {
                    id: node(id),
                    properties: ParagraphProperties::default(),
                    inlines: vec![InlineNode::Run(Run {
                        id: node(id + 1_000),
                        properties: RunProperties::default(),
                        text: (*text).to_owned(),
                    })],
                })
            })
            .collect();
        Document::new(node(9_000), body, Definitions::default()).unwrap()
    }

    fn letter_config() -> PageConfig {
        PageConfig {
            section: SectionId::new(node(9_100)),
            page_size: Size::new(Twip(12_240), Twip(15_840)),
            margin_top: Twip(1_440),
            margin_bottom: Twip(1_440),
            margin_start: Twip(1_440),
            margin_end: Twip(1_440),
            header_distance: Twip(720),
            footer_distance: Twip(720),
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        }
    }

    const WIDTH: Twip = Twip(9_360);

    #[test]
    fn cache_reshapes_only_the_dirty_paragraph() {
        let shaper = SpyShaper::new();
        let mut cache = GalleyCache::new();

        let first = doc(&["alpha", "bravo", "charlie", "delta"]);
        let g1 = build_galley_cached(&first, &shaper, WIDTH, &mut cache, &DirtySet::everything());
        assert_eq!(shaper.calls(), 4, "the first build shapes every paragraph");
        assert_eq!(cache.shaped_last_build(), 4);
        assert_eq!(g1.len(), 4);

        // Edit paragraph 2's text; the transaction reports node 2 dirty.
        let second = doc(&["alpha", "BRAVO-EDITED", "charlie", "delta"]);
        let dirty: DirtySet = [node(2)].into_iter().collect();
        let before = shaper.calls();
        let g2 = build_galley_cached(&second, &shaper, WIDTH, &mut cache, &dirty);

        assert_eq!(
            shaper.calls() - before,
            1,
            "only the dirty paragraph is re-shaped"
        );
        assert_eq!(cache.shaped_last_build(), 1);
        // Untouched paragraphs are reused verbatim from the cache.
        assert_eq!(g2[0], g1[0], "paragraph 1 reused");
        assert_eq!(g2[2], g1[2], "paragraph 3 reused");
        assert_eq!(g2[3], g1[3], "paragraph 4 reused");
        // The edited paragraph is genuinely re-shaped to a different fragment.
        assert_ne!(g2[1], g1[1], "the dirty paragraph changed");
    }

    #[test]
    fn cache_rebuild_equals_a_fresh_uncached_galley() {
        let spy = SpyShaper::new();
        let mut cache = GalleyCache::new();
        let first = doc(&["one", "two", "three"]);
        let _ = build_galley_cached(&first, &spy, WIDTH, &mut cache, &DirtySet::everything());

        let second = doc(&["one", "TWO-CHANGED", "three"]);
        let dirty: DirtySet = [node(2)].into_iter().collect();
        let cached = build_galley_cached(&second, &spy, WIDTH, &mut cache, &dirty);

        // A cache-built galley must equal one built from scratch with no cache.
        let fresh_shaper = SpyShaper::new();
        let fresh = build_galley(&second, &fresh_shaper, WIDTH);
        assert_eq!(
            cached, fresh,
            "the cache never changes the galley's content"
        );
    }

    #[test]
    fn width_change_invalidates_the_whole_cache() {
        let shaper = SpyShaper::new();
        let mut cache = GalleyCache::new();
        let document = doc(&["one", "two", "three"]);
        let _ = build_galley_cached(
            &document,
            &shaper,
            WIDTH,
            &mut cache,
            &DirtySet::everything(),
        );
        let before = shaper.calls();
        // A different wrap width forces every paragraph to re-wrap even with no
        // node marked dirty.
        let _ = build_galley_cached(
            &document,
            &shaper,
            Twip(5_000),
            &mut cache,
            &DirtySet::new(),
        );
        assert_eq!(
            shaper.calls() - before,
            3,
            "a width change re-shapes every paragraph"
        );
    }

    #[test]
    fn first_dirty_fragment_maps_a_node_to_its_fragment() {
        let shaper = SpyShaper::new();
        let document = doc(&["a", "b", "c", "d"]);
        let galley = build_galley(&document, &shaper, WIDTH);
        let dirty: DirtySet = [node(3)].into_iter().collect();
        assert_eq!(first_dirty_fragment(&galley, &dirty), Some(2));
        assert_eq!(
            first_dirty_fragment(&galley, &DirtySet::new()),
            None,
            "no dirty node maps to no fragment"
        );
        assert_eq!(
            first_dirty_fragment(&galley, &DirtySet::everything()),
            Some(0),
            "a whole-document invalidation starts at the first fragment"
        );
    }

    #[test]
    fn reflow_equals_a_full_paginate_of_the_new_galley() {
        let config = letter_config();
        let shaper = SpyShaper::new();
        let mut cache = GalleyCache::new();

        let first = doc(&["a", "b", "c", "d", "e", "f"]);
        let prev_galley =
            build_galley_cached(&first, &shaper, WIDTH, &mut cache, &DirtySet::everything());
        let prev_layout = paginate(&prev_galley, &config);

        let second = doc(&["a", "b", "C-EDITED", "d", "e", "f"]);
        let dirty: DirtySet = [node(3)].into_iter().collect();
        let new_galley = build_galley_cached(&second, &shaper, WIDTH, &mut cache, &dirty);

        let (layout, _stats) =
            reflow_with_stats(&prev_layout, &prev_galley, &new_galley, &dirty, &config);
        assert_eq!(
            layout,
            paginate(&new_galley, &config),
            "reflow must equal a full paginate of the new galley"
        );
    }

    #[test]
    fn paginate_viewport_matches_the_slice_of_a_full_paginate() {
        let config = letter_config();
        let shaper = SpyShaper::new();
        // Enough paragraphs to span several pages (each ~ a page tall).
        let texts: Vec<String> = (0..12).map(|i| format!("para {i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        // Force one page per paragraph via tall single-line paragraphs is awkward
        // through the shaper; instead paginate whatever falls out and window it.
        let document = doc(&refs);
        let galley = build_galley(&document, &shaper, WIDTH);
        let full = paginate(&galley, &config);
        assert!(!full.pages.is_empty());

        let range = PageRange::new(0, full.pages.len());
        let viewport = paginate_viewport(&galley, &config, range);
        assert_eq!(viewport.total_pages, full.pages.len());
        assert_eq!(viewport.pages.len(), full.pages.len());
        let page_height = config.page_size.height.raw();
        for (i, visible) in viewport.pages.iter().enumerate() {
            assert_eq!(visible.page, full.pages[i], "each visible page is verbatim");
            assert_eq!(
                visible.page.number as usize,
                i + 1,
                "the absolute page number is preserved"
            );
            assert_eq!(
                visible.y_offset,
                Twip(page_height * i as i32),
                "pages stack by their full height"
            );
        }
    }

    #[test]
    fn paginate_viewport_returns_only_the_requested_window() {
        let config = letter_config();
        let shaper = SpyShaper::new();
        // Tall paragraphs so the document is many pages: each paragraph is a big
        // run, so a page holds only a few.
        let big = "x".repeat(400);
        let texts: Vec<&str> = vec![big.as_str(); 200];
        let document = doc(&texts);
        let galley = build_galley(&document, &shaper, WIDTH);
        let full = paginate(&galley, &config);
        assert!(
            full.pages.len() >= 3,
            "expected a multi-page document, got {}",
            full.pages.len()
        );

        let window = PageRange::new(1, 3.min(full.pages.len()));
        let viewport = paginate_viewport(&galley, &config, window);
        assert_eq!(
            viewport.total_pages,
            full.pages.len(),
            "total is the whole doc"
        );
        assert_eq!(viewport.pages.len(), window.end - window.start);
        for (k, visible) in viewport.pages.iter().enumerate() {
            let absolute = window.start + k;
            assert_eq!(visible.page, full.pages[absolute]);
        }
    }

    #[test]
    fn viewport_of_windows_a_prebuilt_layout_and_clamps_out_of_range() {
        let config = letter_config();
        let shaper = SpyShaper::new();
        let document = doc(&["a", "b", "c"]);
        let galley = build_galley(&document, &shaper, WIDTH);
        let layout = paginate(&galley, &config);

        // A range past the end clamps to the available pages.
        let viewport = viewport_of(&layout, &config, PageRange::new(0, 999));
        assert_eq!(viewport.total_pages, layout.pages.len());
        assert_eq!(viewport.pages.len(), layout.pages.len());

        // A range wholly past the end yields no pages but the true total.
        let empty = viewport_of(&layout, &config, PageRange::new(50, 60));
        assert_eq!(empty.total_pages, layout.pages.len());
        assert!(empty.pages.is_empty());
    }

    #[test]
    fn dirty_set_basics() {
        let mut set = DirtySet::new();
        assert!(set.is_empty());
        set.mark(node(1)).mark(node(2));
        assert!(!set.is_empty());
        assert_eq!(set.len(), 2);
        assert!(set.contains(node(1)));
        assert!(!set.contains(node(3)));

        let all = DirtySet::everything();
        assert!(all.is_all());
        assert!(all.contains(node(999)));
        assert!(!all.is_empty());
    }
}
