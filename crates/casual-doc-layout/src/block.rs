//! Block-level fragments produced by the block/flow engine.
//!
//! The flow engine ("our PTS", `43-…` §6) turns the document's `BlockNode`s into
//! a galley of [`BlockFragment`]s in flow order and in device-independent units,
//! *before* pagination. The paginator (`43-…` §7) then slices the galley into
//! pages. Fragments are the unit of both dirty-tracking (re-flow) and placement.

use casual_doc_model::NodeId;
use serde::{Deserialize, Serialize};

use crate::text::LineLayout;
use crate::units::Twip;

/// The box edges around a block (margins, border width, padding), in twips. The
/// visual border/shading themselves are emitted as paint items; these are the
/// space they consume during layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct BoxMetrics {
    /// Space above the block (`w:spacing/@before`, top margin/border).
    pub space_before: Twip,
    /// Space below the block (`w:spacing/@after`, bottom margin/border).
    pub space_after: Twip,
    /// Leading (start-edge) indent.
    pub indent_start: Twip,
    /// Trailing (end-edge) indent.
    pub indent_end: Twip,
}

/// A resolved, drawable border edge — the winner of OOXML border-conflict
/// resolution (`docs/38-…#tables`, ECMA-376 §17.4.66). Color is straight-alpha
/// sRGB (packed to avoid a `display` dependency here); width is in twips.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ResolvedEdge {
    /// Edge color (RGBA).
    pub color: [u8; 4],
    /// Line width in twips.
    pub width: Twip,
}

/// The four drawable edges of a block box (paragraph borders, `w:pBdr`). Mirrors
/// [`CellBorders`] but for the leading/trailing/top/bottom edges of a paragraph's
/// content box; `None` = that edge is not drawn. The `w:between`/`w:bar` edges are
/// not modeled here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct BlockBorders {
    /// Top edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<ResolvedEdge>,
    /// Bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<ResolvedEdge>,
    /// Leading (start) edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ResolvedEdge>,
    /// Trailing (end) edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ResolvedEdge>,
}

impl BlockBorders {
    /// Whether no edge is drawn (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The paint-only decoration of a paragraph box: background shading (`w:shd`) and
/// borders (`w:pBdr`), plus the content-box `width` they span (the flowed column
/// width; the start/end indents are subtracted at composition). Layout-neutral —
/// it is consumed by composition, not by the paginator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ParagraphDecor {
    /// Background fill (`w:shd@fill`), RGBA; `None` = no shading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<[u8; 4]>,
    /// Border edges (`w:pBdr`).
    #[serde(default, skip_serializing_if = "BlockBorders::is_empty")]
    pub borders: BlockBorders,
    /// The paragraph's flowed content-box width (twips) — the span shading/borders
    /// cover, before subtracting the start/end indents.
    #[serde(default, skip_serializing_if = "crate::units::Twip::is_zero")]
    pub width: Twip,
}

impl ParagraphDecor {
    /// Whether the paragraph paints no background or border (serializes to
    /// nothing, so a plain paragraph's fragment is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shading.is_none() && self.borders.is_empty()
    }
}

/// The four resolved visible borders of a cell (`None` = fall back to the
/// default grid line). Produced by border-conflict resolution in the flow engine
/// and drawn by composition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CellBorders {
    /// Top edge.
    pub top: Option<ResolvedEdge>,
    /// Leading (start) edge.
    pub start: Option<ResolvedEdge>,
    /// Bottom edge.
    pub bottom: Option<ResolvedEdge>,
    /// Trailing (end) edge.
    pub end: Option<ResolvedEdge>,
}

impl CellBorders {
    /// Whether no edge is resolved (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One cell's laid-out content within a table-row fragment.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CellFragment {
    /// The cell node.
    pub id: NodeId,
    /// Column span (`w:gridSpan`).
    pub grid_span: u32,
    /// The cell's left edge within the row (twips from the row's leading edge).
    pub x: Twip,
    /// The cell's content width (twips) — the span of its grid columns.
    pub width: Twip,
    /// The cell's flowed block fragments.
    pub blocks: Vec<BlockFragment>,
    /// The cell's resolved visible borders (border-conflict winners).
    #[serde(default, skip_serializing_if = "CellBorders::is_empty")]
    pub borders: CellBorders,
    /// The cell's background fill (`w:shd@fill`), RGBA, painted behind its content;
    /// `None` = no shading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<[u8; 4]>,
}

/// A paragraph's page-break behavior, resolved from `ParagraphProperties`
/// (`docs/42-…` §2.5, CSS-Break-3 mapped from DOCX). All-false = no constraints.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct BreakControl {
    /// Force this paragraph to start a new page (`w:pageBreakBefore`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub page_break_before: bool,
    /// Keep on the same page as the next block (`w:keepNext`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub keep_next: bool,
    /// Keep all lines together — do not split across pages (`w:keepLines`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub keep_lines: bool,
    /// Enforce widow/orphan control when split (`w:widowControl`, on by default in
    /// Word).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub widow_control: bool,
}

impl BreakControl {
    /// Whether no break constraint is set (serializes to nothing).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A block fragment in flow order. A paragraph carries its shaped lines; a table
/// row carries its cells and its split-control flags (the paginator honors
/// `can_split`/`header` when a row crosses a page boundary).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum BlockFragment {
    /// A paragraph: its shaped lines plus box metrics.
    Paragraph {
        /// The paragraph node.
        id: NodeId,
        /// Shaped lines.
        lines: LineLayout,
        /// Surrounding box space.
        box_metrics: BoxMetrics,
        /// Page-break behavior (`w:pageBreakBefore`/`w:keepNext`/`w:keepLines`/
        /// `w:widowControl`). The paginator reads it to decide breaks.
        #[serde(default, skip_serializing_if = "BreakControl::is_default")]
        break_control: BreakControl,
        /// Background shading and borders painted behind/around the box
        /// (`w:shd`/`w:pBdr`). Layout-neutral; consumed only by composition.
        #[serde(default, skip_serializing_if = "ParagraphDecor::is_empty")]
        decor: ParagraphDecor,
    },
    /// A table row: cells, whether it may split across a page, and whether it is
    /// a repeated header row (`w:tblHeader`).
    TableRow {
        /// The row node.
        id: NodeId,
        /// The table this row belongs to (so the paginator can group a table's
        /// rows and repeat its header rows on continuation pages).
        table: NodeId,
        /// The row's cells.
        cells: Vec<CellFragment>,
        /// The resolved row height (twips), honoring `w:trHeight` (`atLeast`
        /// grows with content; `exact` is fixed and the content is clipped).
        height: Twip,
        /// May the row split across a page boundary? (`false` = `w:cantSplit`.)
        can_split: bool,
        /// Is this a header row repeated at the top of each page?
        header: bool,
        /// Clip the content to the row height (`w:trHeight` rule `exact`).
        #[serde(default, skip_serializing_if = "core::ops::Not::not")]
        clip: bool,
    },
}

impl BlockFragment {
    /// The natural height this fragment occupies in the galley (before any page
    /// split). Paragraph height includes its box space.
    #[must_use]
    pub fn height(&self) -> Twip {
        match self {
            BlockFragment::Paragraph {
                lines, box_metrics, ..
            } => box_metrics.space_before + lines.height() + box_metrics.space_after,
            // The resolved row height (from the `w:trHeight` rule); the flow
            // engine has already reconciled it with the cells' content height.
            BlockFragment::TableRow { height, .. } => *height,
        }
    }

    /// The tallest cell's stacked content height (twips), independent of the row
    /// height rule — used to reconcile `w:trHeight` against content.
    #[must_use]
    pub fn cells_content_height(cells: &[CellFragment]) -> Twip {
        cells
            .iter()
            .map(|cell| {
                cell.blocks
                    .iter()
                    .map(BlockFragment::height)
                    .fold(Twip::ZERO, |a, h| a + h)
            })
            .max()
            .unwrap_or(Twip::ZERO)
    }

    /// The document node this fragment came from (its anchor for pagination
    /// boundaries and hit-testing).
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        match self {
            BlockFragment::Paragraph { id, .. } | BlockFragment::TableRow { id, .. } => *id,
        }
    }

    /// This fragment's page-break behavior (default — no constraints — for a
    /// table row, whose own split flags are handled separately).
    #[must_use]
    pub fn break_control(&self) -> BreakControl {
        match self {
            BlockFragment::Paragraph { break_control, .. } => *break_control,
            BlockFragment::TableRow { .. } => BreakControl::default(),
        }
    }
}
