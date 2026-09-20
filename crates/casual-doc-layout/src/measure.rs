//! The **measure tier** of the galley — what pagination needs, without glyphs.
//!
//! `docs/113` §3.2: the paginator asks fragments how tall they are and where
//! they may break. It never reads a glyph. But a [`BlockFragment`] carries its
//! [`LineLayout`](crate::text::LineLayout), and a line carries its
//! [`GlyphRun`](crate::text::GlyphRun)s at 96 B each, which is where
//! ~4.7 KB/paragraph of resident memory lives (`docs/111` §2). So the galley
//! splits in two:
//!
//! - the **measure tier** ([`FragmentMeasure`]) — total height, per-line heights,
//!   break opportunities, keep-with-next / keep-lines, and the node id. Resident
//!   for the whole document, on the order of ~100 B/paragraph.
//! - the **paint tier** ([`BlockFragment`]) — the full line layout with glyph
//!   runs, needed only for the pages actually being painted.
//!
//! ## One algorithm, two tiers
//!
//! The dangerous way to do this is to write a second, cheaper paginator for
//! heights. Then a document can paginate differently depending on which tier
//! answered, which is exactly the divergence `docs/113` §5 forbids. Instead the
//! paginator in [`crate::paginate`] is generic over the [`Paginable`] trait
//! below, and both tiers implement it: there is one walk, one set of break
//! rules, one splitter. The tiers differ only in what a placed fragment *is*
//! ([`Paginable::Placed`]) and what a finished page *is*
//! ([`Paginable::Page`]) — a full [`Page`] with painted
//! content for the paint tier, a [`PageOutline`] of boundaries alone for the
//! measure tier.
//!
//! The equality property that follows is asserted directly by the
//! `measure_equals_full_*` tests in [`crate::paginate`].

use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;

use crate::block::BlockFragment;
use crate::block::BreakControl;
use crate::block::CellFragment;
use crate::model::ModelPos;
use crate::page::FlowSpan;
use crate::page::Page;
use crate::page::PlacedFragment;
use crate::paginate::PageConfig;
use crate::units::Rect;
use crate::units::Size;
use crate::units::Twip;

/// One line's entire contribution to pagination: how tall it is, and whether a
/// forced page/column break follows it (`w:br` type `page`/`column`).
///
/// Those are the only two properties of a line the paginator reads. Eight bytes
/// replace a [`Line`](crate::text::Line) (which carries five vectors, a model
/// range and its glyph runs).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineMeasure {
    /// Total line height (`ascent + descent + leading`).
    pub height: Twip,
    /// A forced page/column break follows this line.
    pub page_break_after: bool,
}

/// A table cell's measure tier: its stacked block measures, and the vertical
/// space its margins demand.
///
/// The paginator only ever reads a cell's margins as one number — the sum of
/// `w:tcMar` top/bottom and the split `w:tblCellSpacing` top/bottom — when it
/// decides where a row may be cut, so the measure tier stores that sum rather
/// than the four edges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellMeasure {
    /// The cell's stacked block measures.
    pub blocks: Vec<FragmentMeasure>,
    /// Top + bottom content margins plus top + bottom cell spacing (twips).
    pub vertical_margins: Twip,
}

/// A table row's measure tier. Boxed inside [`FragmentMeasure`] so a paragraph —
/// which a document is nearly entirely made of — is not charged for the row
/// variant's size (`docs/111` §4 stage 1b: a `Vec` pays for its enum's largest
/// variant on every element).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RowMeasure {
    /// The row node.
    pub id: NodeId,
    /// The table this row belongs to.
    pub table: NodeId,
    /// The row's cell measures.
    pub cells: Vec<CellMeasure>,
    /// The resolved row height (twips).
    pub height: Twip,
    /// May the row split across a page boundary? (`false` = `w:cantSplit`.)
    pub can_split: bool,
    /// Is this a header row repeated at the top of each page (`w:tblHeader`)?
    pub header: bool,
    /// Keep this row with the following row (a vertical merge crosses them).
    pub merge_keep_next: bool,
}

/// A block fragment reduced to what pagination reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FragmentMeasure {
    /// A paragraph: its per-line heights and break opportunities, the box space
    /// above and below it, and its break control.
    Paragraph {
        /// Per-line heights and forced breaks. A boxed slice, not a `Vec`: the
        /// line list is immutable once shaped, and this saves 8 B per paragraph.
        lines: Box<[LineMeasure]>,
        /// Space above the block (`w:spacing/@before`).
        space_before: Twip,
        /// Space below the block (`w:spacing/@after`).
        space_after: Twip,
        /// Page-break behavior.
        break_control: BreakControl,
        /// The paragraph node.
        id: NodeId,
    },
    /// A table row.
    TableRow(Box<RowMeasure>),
}

/// Identity and split flags of a table row, as the generic paginator reads them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RowInfo {
    /// The row node.
    pub id: NodeId,
    /// The table the row belongs to.
    pub table: NodeId,
    /// May the row split across a page boundary?
    pub can_split: bool,
    /// Is this a repeated header row?
    pub header: bool,
}

/// A page's boundaries without its content — what the measure tier emits in
/// place of a [`Page`].
///
/// Every field is a field [`Page`] also has and carries the same value, which is
/// what makes "the measure tier paginates identically" a mechanical assertion
/// rather than a judgement call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageOutline {
    /// 1-based page number.
    pub number: u32,
    /// The section whose geometry produced this page.
    pub section: SectionId,
    /// Full page size (twips).
    pub page_size: Size,
    /// The body content area.
    pub content_area: Rect,
    /// First model position on the page.
    pub start: ModelPos,
    /// Last model position on the page.
    pub end: ModelPos,
    /// The half-open galley span the page covers.
    pub flow: FlowSpan,
}

impl PageOutline {
    /// The outline of an already-built [`Page`] — the projection the equality
    /// tests compare a measure-tier page against.
    #[must_use]
    pub fn of(page: &Page) -> Self {
        Self {
            number: page.number,
            section: page.section,
            page_size: page.page_size,
            content_area: page.content_area,
            start: page.start,
            end: page.end,
            flow: page.flow,
        }
    }
}

/// A measure-tier pagination: the page boundaries of the whole document, and
/// the checkpoints from which any page can be re-derived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeasureLayout {
    /// Every page's boundaries, in order.
    pub pages: Vec<PageOutline>,
    /// Resumable pagination checkpoints (see [`crate::paginate::Checkpoint`]).
    pub checkpoints: Vec<crate::paginate::Checkpoint>,
}

impl FragmentMeasure {
    /// Projects a paint-tier fragment onto the measure tier, discarding glyphs.
    ///
    /// This is the adapter that measures an *existing* galley. Building
    /// measures directly as paragraphs are shaped — so the full galley never
    /// becomes resident at all — is the flow-engine half of the change and is
    /// not landed yet; `docs/113` §6 step 4 records what is left.
    #[must_use]
    pub fn of(fragment: &BlockFragment) -> Self {
        match fragment {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
                ..
            } => Self::Paragraph {
                lines: lines
                    .lines
                    .iter()
                    .map(|line| LineMeasure {
                        height: line.height,
                        page_break_after: line.page_break_after,
                    })
                    .collect(),
                space_before: box_metrics.space_before,
                space_after: box_metrics.space_after,
                break_control: *break_control,
                id: *id,
            },
            BlockFragment::TableRow {
                id,
                table,
                cells,
                height,
                can_split,
                header,
                merge_keep_next,
                ..
            } => Self::TableRow(Box::new(RowMeasure {
                id: *id,
                table: *table,
                cells: cells.iter().map(CellMeasure::of).collect(),
                height: *height,
                can_split: *can_split,
                header: *header,
                merge_keep_next: *merge_keep_next,
            })),
        }
    }
}

impl CellMeasure {
    /// Projects a paint-tier cell onto the measure tier.
    #[must_use]
    pub fn of(cell: &CellFragment) -> Self {
        Self {
            blocks: cell.blocks.iter().map(FragmentMeasure::of).collect(),
            vertical_margins: Twip(
                cell.margins
                    .top
                    .raw()
                    .saturating_add(cell.margins.bottom.raw())
                    .saturating_add(cell.cell_spacing.top.raw())
                    .saturating_add(cell.cell_spacing.bottom.raw()),
            ),
        }
    }
}

/// The measure tier of a whole galley.
#[must_use]
pub fn measure_galley(fragments: &[BlockFragment]) -> Vec<FragmentMeasure> {
    fragments.iter().map(FragmentMeasure::of).collect()
}

/// A block fragment the paginator can place, in either tier.
///
/// Every method is something [`crate::paginate`]'s walk actually asks of a
/// fragment. Keeping the list minimal is the point: it is the proof surface for
/// "pagination needs heights, not glyphs" — if a glyph-bearing accessor ever has
/// to be added here, the measure tier has stopped being sufficient and the claim
/// in `docs/113` §3.2 has to be revisited.
pub trait Paginable: Clone + Sized {
    /// The cell type carried by a table row in this tier.
    type Cell: PaginableCell<Fragment = Self>;
    /// What placing a fragment on a page records.
    type Placed;
    /// What a finished page is in this tier.
    type Page;

    /// The natural height this fragment occupies in the galley, before any page
    /// split (a paragraph's includes its box space).
    fn height(&self) -> Twip;

    /// This fragment's page-break behavior.
    fn break_control(&self) -> BreakControl;

    /// The document node this fragment came from.
    fn node_id(&self) -> NodeId;

    /// Whether this row participates in a multi-row vertical-merge keep group.
    fn is_vertical_merge_row(&self) -> bool;

    /// Row identity and split flags, or `None` for a paragraph.
    fn row_info(&self) -> Option<RowInfo>;

    /// The row's cells (empty for a paragraph).
    fn cells(&self) -> &[Self::Cell];

    /// How many lines this paragraph has (`0` for a row).
    fn line_count(&self) -> usize;

    /// The height of line `index`.
    fn line_height(&self, index: usize) -> Twip;

    /// Whether a forced page/column break follows line `index`.
    fn line_page_break_after(&self, index: usize) -> bool;

    /// Space above the block (`w:spacing/@before`).
    fn space_before(&self) -> Twip;

    /// Space below the block (`w:spacing/@after`).
    fn space_after(&self) -> Twip;

    /// A chunk of this paragraph covering `range`, re-based to sit at the
    /// fragment top and keeping `space_before`/`space_after` only on the
    /// head/tail chunk.
    fn slice_paragraph(&self, range: core::ops::Range<usize>) -> Self;

    /// A continuation piece of this row carrying `cells` and occupying `height`.
    fn row_chunk(&self, cells: Vec<Self::Cell>, height: Twip) -> Self;

    /// Records this fragment as placed at `rect` on a page of `section`.
    fn into_placed(self, rect: Rect, section: SectionId) -> Self::Placed;

    /// Assembles page `index` (0-based) from the fragments placed on it.
    fn build_page(
        index: usize,
        config: &PageConfig,
        content: Rect,
        placed: Vec<Self::Placed>,
        flow: FlowSpan,
    ) -> Self::Page;

    /// Whether any line carries a forced page/column break.
    fn any_line_page_break_after(&self) -> bool {
        (0..self.line_count()).any(|i| self.line_page_break_after(i))
    }
}

/// A table cell the paginator can cut, in either tier.
pub trait PaginableCell: Clone + Sized {
    /// The block fragment type this cell stacks.
    type Fragment: Paginable<Cell = Self>;

    /// The cell's stacked block fragments.
    fn blocks(&self) -> &[Self::Fragment];

    /// The same cell carrying `blocks` instead.
    fn with_blocks(&self, blocks: Vec<Self::Fragment>) -> Self;

    /// Top + bottom margins and cell spacing (twips) — the vertical space the
    /// cell box costs beyond its content.
    fn vertical_margins(&self) -> Twip;

    /// The cell's content height plus [`vertical_margins`](Self::vertical_margins).
    fn occupied_height(&self) -> Twip;
}

/// The tallest cell's occupied height — a row's minimum content height,
/// independent of the `w:trHeight` rule.
#[must_use]
pub fn cells_content_height<C: PaginableCell>(cells: &[C]) -> Twip {
    cells
        .iter()
        .map(PaginableCell::occupied_height)
        .max()
        .unwrap_or(Twip::ZERO)
}

impl Paginable for BlockFragment {
    type Cell = CellFragment;
    type Placed = PlacedFragment;
    type Page = Page;

    fn height(&self) -> Twip {
        BlockFragment::height(self)
    }

    fn break_control(&self) -> BreakControl {
        BlockFragment::break_control(self)
    }

    fn node_id(&self) -> NodeId {
        BlockFragment::node_id(self)
    }

    fn is_vertical_merge_row(&self) -> bool {
        BlockFragment::is_vertical_merge_row(self)
    }

    fn row_info(&self) -> Option<RowInfo> {
        match self {
            BlockFragment::Paragraph { .. } => None,
            BlockFragment::TableRow {
                id,
                table,
                can_split,
                header,
                ..
            } => Some(RowInfo {
                id: *id,
                table: *table,
                can_split: *can_split,
                header: *header,
            }),
        }
    }

    fn cells(&self) -> &[CellFragment] {
        match self {
            BlockFragment::Paragraph { .. } => &[],
            BlockFragment::TableRow { cells, .. } => cells,
        }
    }

    fn line_count(&self) -> usize {
        match self {
            BlockFragment::Paragraph { lines, .. } => lines.lines.len(),
            BlockFragment::TableRow { .. } => 0,
        }
    }

    fn line_height(&self, index: usize) -> Twip {
        match self {
            BlockFragment::Paragraph { lines, .. } => lines.lines[index].height,
            BlockFragment::TableRow { .. } => Twip::ZERO,
        }
    }

    fn line_page_break_after(&self, index: usize) -> bool {
        match self {
            BlockFragment::Paragraph { lines, .. } => lines.lines[index].page_break_after,
            BlockFragment::TableRow { .. } => false,
        }
    }

    fn space_before(&self) -> Twip {
        match self {
            BlockFragment::Paragraph { box_metrics, .. } => box_metrics.space_before,
            BlockFragment::TableRow { .. } => Twip::ZERO,
        }
    }

    fn space_after(&self) -> Twip {
        match self {
            BlockFragment::Paragraph { box_metrics, .. } => box_metrics.space_after,
            BlockFragment::TableRow { .. } => Twip::ZERO,
        }
    }

    fn slice_paragraph(&self, range: core::ops::Range<usize>) -> Self {
        match self {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
                decor,
            } => crate::paginate::slice_paragraph(
                *id,
                lines,
                *box_metrics,
                *break_control,
                *decor,
                range,
            ),
            BlockFragment::TableRow { .. } => self.clone(),
        }
    }

    fn row_chunk(&self, cells: Vec<CellFragment>, height: Twip) -> Self {
        match self.row_info() {
            Some(info) => crate::paginate::make_row_chunk(
                info.id,
                info.table,
                cells,
                height,
                info.can_split,
                info.header,
            ),
            None => self.clone(),
        }
    }

    fn into_placed(self, rect: Rect, section: SectionId) -> PlacedFragment {
        PlacedFragment {
            fragment: self,
            rect,
            section: Some(section),
        }
    }

    fn build_page(
        index: usize,
        config: &PageConfig,
        content: Rect,
        placed: Vec<PlacedFragment>,
        flow: FlowSpan,
    ) -> Page {
        crate::paginate::build_page(index, config, content, placed, flow)
    }
}

impl PaginableCell for CellFragment {
    type Fragment = BlockFragment;

    fn blocks(&self) -> &[BlockFragment] {
        &self.blocks
    }

    fn with_blocks(&self, blocks: Vec<BlockFragment>) -> Self {
        CellFragment {
            blocks,
            ..self.clone()
        }
    }

    fn vertical_margins(&self) -> Twip {
        Twip(
            self.margins
                .top
                .raw()
                .saturating_add(self.margins.bottom.raw())
                .saturating_add(self.cell_spacing.top.raw())
                .saturating_add(self.cell_spacing.bottom.raw()),
        )
    }

    fn occupied_height(&self) -> Twip {
        CellFragment::occupied_height(self)
    }
}

impl Paginable for FragmentMeasure {
    type Cell = CellMeasure;
    /// A placed measure records only the node it came from — enough to rebuild
    /// the page's start/end model positions, and nothing else.
    type Placed = NodeId;
    type Page = PageOutline;

    fn height(&self) -> Twip {
        match self {
            FragmentMeasure::Paragraph {
                lines,
                space_before,
                space_after,
                ..
            } => {
                let content = lines.iter().fold(Twip::ZERO, |acc, line| acc + line.height);
                *space_before + content + *space_after
            }
            FragmentMeasure::TableRow(row) => row.height,
        }
    }

    fn break_control(&self) -> BreakControl {
        match self {
            FragmentMeasure::Paragraph { break_control, .. } => *break_control,
            FragmentMeasure::TableRow(row) => BreakControl {
                keep_next: row.merge_keep_next,
                ..BreakControl::default()
            },
        }
    }

    fn node_id(&self) -> NodeId {
        match self {
            FragmentMeasure::Paragraph { id, .. } => *id,
            FragmentMeasure::TableRow(row) => row.id,
        }
    }

    fn is_vertical_merge_row(&self) -> bool {
        matches!(self, FragmentMeasure::TableRow(row) if row.merge_keep_next)
    }

    fn row_info(&self) -> Option<RowInfo> {
        match self {
            FragmentMeasure::Paragraph { .. } => None,
            FragmentMeasure::TableRow(row) => Some(RowInfo {
                id: row.id,
                table: row.table,
                can_split: row.can_split,
                header: row.header,
            }),
        }
    }

    fn cells(&self) -> &[CellMeasure] {
        match self {
            FragmentMeasure::Paragraph { .. } => &[],
            FragmentMeasure::TableRow(row) => &row.cells,
        }
    }

    fn line_count(&self) -> usize {
        match self {
            FragmentMeasure::Paragraph { lines, .. } => lines.len(),
            FragmentMeasure::TableRow(_) => 0,
        }
    }

    fn line_height(&self, index: usize) -> Twip {
        match self {
            FragmentMeasure::Paragraph { lines, .. } => lines[index].height,
            FragmentMeasure::TableRow(_) => Twip::ZERO,
        }
    }

    fn line_page_break_after(&self, index: usize) -> bool {
        match self {
            FragmentMeasure::Paragraph { lines, .. } => lines[index].page_break_after,
            FragmentMeasure::TableRow(_) => false,
        }
    }

    fn space_before(&self) -> Twip {
        match self {
            FragmentMeasure::Paragraph { space_before, .. } => *space_before,
            FragmentMeasure::TableRow(_) => Twip::ZERO,
        }
    }

    fn space_after(&self) -> Twip {
        match self {
            FragmentMeasure::Paragraph { space_after, .. } => *space_after,
            FragmentMeasure::TableRow(_) => Twip::ZERO,
        }
    }

    fn slice_paragraph(&self, range: core::ops::Range<usize>) -> Self {
        match self {
            FragmentMeasure::Paragraph {
                lines,
                space_before,
                space_after,
                break_control,
                id,
            } => {
                let is_head = range.start == 0;
                let is_tail = range.end == lines.len();
                FragmentMeasure::Paragraph {
                    lines: lines[range].into(),
                    space_before: if is_head { *space_before } else { Twip::ZERO },
                    space_after: if is_tail { *space_after } else { Twip::ZERO },
                    break_control: *break_control,
                    id: *id,
                }
            }
            FragmentMeasure::TableRow(_) => self.clone(),
        }
    }

    fn row_chunk(&self, cells: Vec<CellMeasure>, height: Twip) -> Self {
        match self {
            FragmentMeasure::Paragraph { .. } => self.clone(),
            FragmentMeasure::TableRow(row) => FragmentMeasure::TableRow(Box::new(RowMeasure {
                id: row.id,
                table: row.table,
                cells,
                height,
                can_split: row.can_split,
                header: row.header,
                // Mirrors `make_row_chunk`: a continuation piece never carries
                // the merge keep-with-next flag.
                merge_keep_next: false,
            })),
        }
    }

    fn into_placed(self, _rect: Rect, _section: SectionId) -> NodeId {
        self.node_id()
    }

    fn build_page(
        index: usize,
        config: &PageConfig,
        content: Rect,
        placed: Vec<NodeId>,
        flow: FlowSpan,
    ) -> PageOutline {
        // Mirrors `paginate::build_page`: the page's model span runs from the
        // first placed fragment's node to the last's.
        let start = ModelPos::new(*placed.first().expect("a page has content"), 0);
        let end = ModelPos::new(*placed.last().expect("a page has content"), 0);
        PageOutline {
            number: (index + 1) as u32,
            section: config.section,
            page_size: config.page_size,
            content_area: content,
            start,
            end,
            flow,
        }
    }
}

impl PaginableCell for CellMeasure {
    type Fragment = FragmentMeasure;

    fn blocks(&self) -> &[FragmentMeasure] {
        &self.blocks
    }

    fn with_blocks(&self, blocks: Vec<FragmentMeasure>) -> Self {
        CellMeasure {
            blocks,
            vertical_margins: self.vertical_margins,
        }
    }

    fn vertical_margins(&self) -> Twip {
        self.vertical_margins
    }

    fn occupied_height(&self) -> Twip {
        self.blocks
            .iter()
            .map(Paginable::height)
            .fold(self.vertical_margins, |a, h| a + h)
    }
}

// --- Streaming the measure tier out of the flow engine ---------------------

/// Where [`crate::flow`] puts the block fragments it produces.
///
/// `docs/113` §6 step 4. Until this existed the flow engine could only append
/// to a `Vec<BlockFragment>`, so the *only* way to obtain the measure tier was
/// to build the whole paint tier and project it ([`measure_galley`]) — which
/// made the measure tier's small footprint a projection rather than a peak. A
/// sink lets the engine hand each fragment somewhere that keeps only what it
/// needs, so the glyph-bearing form of a paragraph can be dropped as soon as
/// the next one is shaped.
///
/// ## The one-fragment lookback contract
///
/// The flow engine mutates a fragment it has already pushed in exactly one
/// place: `w:contextualSpacing` collapses the gap between two adjacent
/// same-style paragraphs, which zeroes the *current* fragment's space-before
/// and the *previous* fragment's space-after (ECMA-376 §17.3.1.9). A paragraph
/// contributes exactly one fragment and the collapse runs immediately after
/// the push, so the reach of that mutation is the last two fragments and no
/// more.
///
/// `zero_space_before` and `zero_space_after` are therefore specified to be
/// called only with an index in `len() - 2 ..= len() - 1`, and [`MeasureSink`]
/// — which cannot reach further back, because it has already discarded the
/// glyphs — **panics** rather than silently applying the collapse to the wrong
/// paragraph.
pub trait GalleySink {
    /// Appends a fragment.
    fn push(&mut self, fragment: BlockFragment);

    /// How many fragments have been pushed.
    fn len(&self) -> usize;

    /// Whether nothing has been pushed.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The stacked height of everything pushed so far — the flow-relative top
    /// of the next fragment, which is what a square-wrap float's clearance is
    /// measured from.
    fn stacked_height(&self) -> Twip;

    /// Zeroes `w:spacing/@before` on the fragment at `index` (a no-op for a
    /// table row). See the lookback contract on [`GalleySink`].
    fn zero_space_before(&mut self, index: usize);

    /// Zeroes `w:spacing/@after` on the fragment at `index` (a no-op for a
    /// table row). See the lookback contract on [`GalleySink`].
    fn zero_space_after(&mut self, index: usize);

    /// Records that a top-level block of the flowed sequence starts at the next
    /// fragment. Inert for the paint tier; [`MeasureSink`] keeps the marks so a
    /// window that has to re-flow galley fragment *f* knows which block to
    /// start flowing from.
    fn mark_block(&mut self) {}
}

/// The paint tier's sink: the galley itself. Every behavior here is what the
/// flow engine did inline before [`GalleySink`] existed, including the
/// deliberately re-summed [`stacked_height`](GalleySink::stacked_height) — it
/// is asked for only when a float is actually anchored, which is rare.
impl GalleySink for Vec<BlockFragment> {
    fn push(&mut self, fragment: BlockFragment) {
        Vec::push(self, fragment);
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn stacked_height(&self) -> Twip {
        self.iter()
            .map(BlockFragment::height)
            .fold(Twip::ZERO, |a, h| a + h)
    }

    fn zero_space_before(&mut self, index: usize) {
        if let BlockFragment::Paragraph { box_metrics, .. } = &mut self[index] {
            box_metrics.space_before = Twip::ZERO;
        }
    }

    fn zero_space_after(&mut self, index: usize) {
        if let BlockFragment::Paragraph { box_metrics, .. } = &mut self[index] {
            box_metrics.space_after = Twip::ZERO;
        }
    }
}

/// How many just-pushed fragments a [`MeasureSink`] keeps in their paint form.
///
/// Two, which is exactly the reach of the `w:contextualSpacing` collapse
/// documented on [`GalleySink`]: the fragment being pushed and its
/// predecessor. Everything older is projected onto the measure tier and its
/// glyphs dropped.
const MEASURE_SINK_LOOKBACK: usize = 2;

/// The measure tier's sink: projects each fragment as soon as it can no longer
/// be mutated, and drops the glyph-bearing form.
///
/// This is what makes `docs/113` §3.2 true of *peak* memory and not only of
/// resident memory. At most `MEASURE_SINK_LOOKBACK` (two) shaped paragraphs exist
/// at once, whatever the document's length, so the flow engine's own
/// high-water mark stops scaling with the document.
///
/// It is not a second flow engine and it re-derives nothing: the fragments it
/// sees are the fragments [`build_galley`](crate::flow::build_galley) would
/// have returned, in order, and [`FragmentMeasure::of`] is the same projection
/// [`measure_galley`] applies to a finished galley. That equality is asserted
/// directly — see the `streaming_*` tests in
/// `crates/casual-doc-layout/tests/windowed_layout.rs`.
#[derive(Debug, Default)]
pub struct MeasureSink {
    /// Fragments already projected, in galley order.
    measures: Vec<FragmentMeasure>,
    /// Galley index of `pending[0]`.
    pending_base: usize,
    /// The tail still open to the one-fragment lookback, oldest first.
    pending: Vec<BlockFragment>,
    /// Stacked height of everything already projected.
    committed_height: Twip,
    /// Galley index of the first fragment of each top-level block.
    block_marks: Vec<u32>,
}

impl MeasureSink {
    /// An empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many fragments are still held in their **shaped** (glyph-bearing)
    /// form — the sink's live lookback.
    ///
    /// Never more than `MEASURE_SINK_LOOKBACK` (two), whatever the document's
    /// length. That bound is the memory claim of `docs/113` §6 step 4, so it
    /// is observable rather than only asserted in prose.
    #[must_use]
    pub fn shaped_len(&self) -> usize {
        self.pending.len()
    }

    /// Projects everything still pending and returns the finished tier.
    #[must_use]
    pub fn finish(mut self) -> Vec<FragmentMeasure> {
        self.drain_to(0);
        self.measures
    }

    /// Projects everything still pending and returns the tier together with
    /// the galley index each top-level block started at.
    #[must_use]
    pub fn finish_with_block_marks(mut self) -> (Vec<FragmentMeasure>, Vec<u32>) {
        self.drain_to(0);
        (self.measures, self.block_marks)
    }

    /// Projects pending fragments until at most `keep` remain.
    fn drain_to(&mut self, keep: usize) {
        while self.pending.len() > keep {
            let fragment = self.pending.remove(0);
            self.committed_height = self.committed_height + fragment.height();
            self.measures.push(FragmentMeasure::of(&fragment));
            self.pending_base += 1;
        }
    }

    /// The pending slot for galley index `index`, or a panic naming the
    /// violated lookback contract.
    fn pending_mut(&mut self, index: usize) -> &mut BlockFragment {
        let slot = index
            .checked_sub(self.pending_base)
            .filter(|slot| *slot < self.pending.len());
        let Some(slot) = slot else {
            panic!(
                "the measure sink was asked to mutate galley fragment {index}, which it has \
                 already projected and dropped; the flow engine's retroactive mutation must \
                 stay within the last {MEASURE_SINK_LOOKBACK} fragments (see `GalleySink`)"
            );
        };
        &mut self.pending[slot]
    }
}

impl GalleySink for MeasureSink {
    fn push(&mut self, fragment: BlockFragment) {
        self.pending.push(fragment);
        self.drain_to(MEASURE_SINK_LOOKBACK);
    }

    fn len(&self) -> usize {
        self.pending_base + self.pending.len()
    }

    fn stacked_height(&self) -> Twip {
        self.pending
            .iter()
            .map(BlockFragment::height)
            .fold(self.committed_height, |a, h| a + h)
    }

    fn zero_space_before(&mut self, index: usize) {
        if let BlockFragment::Paragraph { box_metrics, .. } = self.pending_mut(index) {
            box_metrics.space_before = Twip::ZERO;
        }
    }

    fn zero_space_after(&mut self, index: usize) {
        if let BlockFragment::Paragraph { box_metrics, .. } = self.pending_mut(index) {
            box_metrics.space_after = Twip::ZERO;
        }
    }

    fn mark_block(&mut self) {
        self.block_marks.push(self.len() as u32);
    }
}

/// A sink that keeps the paint tier for **one galley window** and drops
/// everything else (`docs/113` §6 step 4).
///
/// Same lookback contract and same shape as [`MeasureSink`] — the difference
/// is what happens to a fragment that leaves the lookback: the measure sink
/// projects it, this one keeps it if it falls inside the window and drops it
/// otherwise. So re-flowing a long document to paint page 40,000 costs the
/// *shaping* of everything before it (unavoidable when the flow state is not
/// resumable) but never its *storage*.
///
/// Indices are galley-absolute: `offset` is the absolute index the flow that
/// feeds this sink starts at, so a caller that begins mid-document still names
/// its window in the same coordinates the checkpoints use.
#[derive(Debug)]
pub struct WindowSink {
    /// Absolute galley index this flow started at.
    offset: usize,
    /// The absolute window to keep, half-open.
    keep: core::ops::Range<usize>,
    /// Kept fragments, in order.
    out: Vec<BlockFragment>,
    /// Absolute index of `out[0]`, once something has been kept.
    first_kept: Option<usize>,
    /// Galley index of `pending[0]`, relative to this flow's start.
    pending_base: usize,
    /// The tail still open to the one-fragment lookback, oldest first.
    pending: Vec<BlockFragment>,
    /// Stacked height of everything that has left the lookback.
    committed_height: Twip,
}

impl WindowSink {
    /// A sink for the absolute window `keep`, fed by a flow that starts at
    /// absolute galley index `offset`.
    #[must_use]
    pub fn new(offset: usize, keep: core::ops::Range<usize>) -> Self {
        Self {
            offset,
            keep,
            out: Vec::new(),
            first_kept: None,
            pending_base: 0,
            pending: Vec::new(),
            committed_height: Twip::ZERO,
        }
    }

    /// The absolute galley index of the first kept fragment, or the window
    /// start when nothing was kept.
    #[must_use]
    pub fn base(&self) -> u32 {
        self.first_kept.unwrap_or(self.keep.start) as u32
    }

    /// Finishes the flow and returns the kept fragments with the absolute
    /// index of the first of them.
    #[must_use]
    pub fn finish(mut self) -> (Vec<BlockFragment>, u32) {
        self.drain_to(0);
        let base = self.base();
        (self.out, base)
    }

    fn drain_to(&mut self, keep: usize) {
        while self.pending.len() > keep {
            let fragment = self.pending.remove(0);
            self.committed_height = self.committed_height + fragment.height();
            let absolute = self.offset + self.pending_base;
            self.pending_base += 1;
            if self.keep.contains(&absolute) {
                if self.first_kept.is_none() {
                    self.first_kept = Some(absolute);
                }
                self.out.push(fragment);
            }
        }
    }

    fn pending_mut(&mut self, index: usize) -> &mut BlockFragment {
        let local = index.checked_sub(self.offset);
        let slot = local
            .and_then(|local| local.checked_sub(self.pending_base))
            .filter(|slot| *slot < self.pending.len());
        let Some(slot) = slot else {
            panic!(
                "the window sink was asked to mutate galley fragment {index}, which it has \
                 already passed; the flow engine's retroactive mutation must stay within \
                 the last {MEASURE_SINK_LOOKBACK} fragments (see `GalleySink`)"
            );
        };
        &mut self.pending[slot]
    }
}

impl GalleySink for WindowSink {
    fn push(&mut self, fragment: BlockFragment) {
        self.pending.push(fragment);
        self.drain_to(MEASURE_SINK_LOOKBACK);
    }

    fn len(&self) -> usize {
        // Flow-relative, like every other sink: the engine's own indices count
        // from the start of the sequence it was handed.
        self.pending_base + self.pending.len()
    }

    fn stacked_height(&self) -> Twip {
        self.pending
            .iter()
            .map(BlockFragment::height)
            .fold(self.committed_height, |a, h| a + h)
    }

    fn zero_space_before(&mut self, index: usize) {
        let absolute = self.offset + index;
        if let BlockFragment::Paragraph { box_metrics, .. } = self.pending_mut(absolute) {
            box_metrics.space_before = Twip::ZERO;
        }
    }

    fn zero_space_after(&mut self, index: usize) {
        let absolute = self.offset + index;
        if let BlockFragment::Paragraph { box_metrics, .. } = self.pending_mut(absolute) {
            box_metrics.space_after = Twip::ZERO;
        }
    }
}
