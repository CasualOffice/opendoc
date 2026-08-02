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

/// A backend-independent visible border pattern.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub enum BorderPattern {
    /// One continuous band.
    #[default]
    Solid,
    /// Two parallel bands inside the authored total width.
    Double,
    /// Repeating square dots.
    Dotted,
    /// Repeating long dashes.
    Dashed,
    /// Alternating dot and dash.
    DotDash,
    /// Alternating two dots and one dash.
    DotDotDash,
}

impl BorderPattern {
    /// Whether this is the compact default representation.
    #[must_use]
    pub fn is_solid(&self) -> bool {
        matches!(self, Self::Solid)
    }
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
    /// Visible line pattern.
    #[serde(default, skip_serializing_if = "BorderPattern::is_solid")]
    pub pattern: BorderPattern,
}

/// One independently resolved interval along a horizontal cell side.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ResolvedBorderSegment {
    /// Offset from the cell side's leading edge.
    pub offset: Twip,
    /// Length along the side.
    pub length: Twip,
    /// Border-conflict winner for this interval.
    pub edge: ResolvedEdge,
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

/// The four resolved visible borders of a cell (`None` = draw no edge). Produced
/// by border-conflict resolution in the flow engine and drawn by composition.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CellBorders {
    /// Top edge.
    pub top: Option<ResolvedEdge>,
    /// Leading (start) edge.
    pub start: Option<ResolvedEdge>,
    /// Bottom edge.
    pub bottom: Option<ResolvedEdge>,
    /// Trailing (end) edge.
    pub end: Option<ResolvedEdge>,
    /// Independently resolved intervals along the top side. When present, these
    /// override the whole-side `top` fallback during composition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_segments: Vec<ResolvedBorderSegment>,
    /// Independently resolved intervals along the bottom side. When present,
    /// these override the whole-side `bottom` fallback during composition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bottom_segments: Vec<ResolvedBorderSegment>,
}

impl CellBorders {
    /// Whether no edge is resolved (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A cell's resolved content margins (`w:tcMar` ⊕ `w:tblCellMar` ⊕ Word's
/// defaults), in twips — the inset from each cell edge to its content box. Content
/// is flowed at `width − start − end` and offset by `start`/`top`; the top+bottom
/// margins also count toward the row's content height (`docs/38-…#tables`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CellContentMargins {
    /// Top inset.
    pub top: Twip,
    /// Leading (start) inset.
    pub start: Twip,
    /// Bottom inset.
    pub bottom: Twip,
    /// Trailing (end) inset.
    pub end: Twip,
}

impl CellContentMargins {
    /// Whether every inset is zero (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The separated-cell gap allocated around one physical cell box. Horizontal
/// values are also retained after `x`/`width` have been inset so composition can
/// recover the table-grid slot for its distinct outer-border layer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CellBoxSpacing {
    /// Gap between the logical/physical top of the row and the cell box.
    pub top: Twip,
    /// Gap between the physical leading side of the grid slot and the cell box.
    pub start: Twip,
    /// Gap between the cell box and the logical/physical bottom of the row.
    pub bottom: Twip,
    /// Gap between the cell box and the physical trailing side of the grid slot.
    pub end: Twip,
}

impl CellBoxSpacing {
    /// Whether this is collapsed-cell geometry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A cell's vertical content alignment (`w:vAlign`) — how the cell's stacked
/// content sits within the (possibly taller) row box. `Top` is Word's default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum CellVAlign {
    /// Content sits at the top of the cell (Word's default).
    #[default]
    Top,
    /// Content is centered vertically within the cell.
    Center,
    /// Content sits at the bottom of the cell.
    Bottom,
}

/// A flowed cell's role in a validated vertical merge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum CellVerticalMerge {
    /// The cell is not part of a conforming vertical merge.
    #[default]
    None,
    /// The content-owning first cell. `height` is the final height of every row
    /// covered by the merge.
    Restart {
        /// Merged cell-box height in twips.
        height: Twip,
    },
    /// A subsequent physical cell covered by the restart cell above it.
    Continue,
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
    /// The physical cell-border-box width (twips). With non-zero cell spacing,
    /// this is the grid-column span minus its split start/end gaps.
    pub width: Twip,
    /// The split `w:tblCellSpacing` gap surrounding this cell's border box.
    #[serde(default, skip_serializing_if = "CellBoxSpacing::is_empty")]
    pub cell_spacing: CellBoxSpacing,
    /// The cell's flowed block fragments.
    pub blocks: Vec<BlockFragment>,
    /// The cell's resolved content margins (`w:tcMar`/`w:tblCellMar`). Content is
    /// inset by these; composition offsets the flowed blocks accordingly.
    #[serde(default, skip_serializing_if = "CellContentMargins::is_empty")]
    pub margins: CellContentMargins,
    /// The cell's vertical content alignment (`w:vAlign`); `Top` = Word's default.
    #[serde(default, skip_serializing_if = "CellVAlign::is_top")]
    pub vertical_alignment: CellVAlign,
    /// Validated vertical-merge role. Continuations preserve their model identity
    /// but own no independently painted box or content.
    #[serde(default, skip_serializing_if = "CellVerticalMerge::is_none")]
    pub vertical_merge: CellVerticalMerge,
    /// The cell's resolved visible borders (border-conflict winners).
    #[serde(default, skip_serializing_if = "CellBorders::is_empty")]
    pub borders: CellBorders,
    /// Table-perimeter borders painted on the enclosing grid slot. Non-empty
    /// only for separated-cell rows, where table and cell borders both remain
    /// visible instead of collapsing to one winner.
    #[serde(default, skip_serializing_if = "CellBorders::is_empty")]
    pub table_borders: CellBorders,
    /// The cell's background fill (`w:shd@fill`), RGBA, painted behind its content;
    /// `None` = no shading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<[u8; 4]>,
}

impl CellVAlign {
    /// Whether this is the default `Top` alignment (serializes to nothing).
    #[must_use]
    pub fn is_top(&self) -> bool {
        matches!(self, CellVAlign::Top)
    }
}

impl CellVerticalMerge {
    /// Whether this cell is outside a conforming vertical merge.
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, CellVerticalMerge::None)
    }
}

impl CellFragment {
    /// The physical height of this cell's inset border box. A merge restart spans
    /// every covered row; ordinary cells and continuations use their current row
    /// height, with separated-cell top/bottom gaps removed in both cases.
    #[must_use]
    pub fn box_height(&self, row_height: Twip) -> Twip {
        let height = match self.vertical_merge {
            CellVerticalMerge::Restart { height } => height,
            CellVerticalMerge::None | CellVerticalMerge::Continue => row_height,
        };
        Twip(
            height
                .raw()
                .saturating_sub(self.cell_spacing.top.raw())
                .saturating_sub(self.cell_spacing.bottom.raw())
                .max(1),
        )
    }

    /// The stacked height of the cell's flowed content (twips), excluding margins.
    #[must_use]
    pub fn content_height(&self) -> Twip {
        self.blocks
            .iter()
            .map(BlockFragment::height)
            .fold(Twip::ZERO, |a, h| a + h)
    }

    /// The cell's content height plus its top and bottom margins (twips) — the
    /// vertical space the cell demands of its row.
    #[must_use]
    pub fn occupied_height(&self) -> Twip {
        self.cell_spacing.top
            + self.margins.top
            + self.content_height()
            + self.margins.bottom
            + self.cell_spacing.bottom
    }

    /// The vertical offset (twips) from the cell's top edge to the top of its
    /// content, once the row height is known: the top margin plus the `w:vAlign`
    /// share of the leftover slack (`Top` → 0, `Center` → half, `Bottom` → all).
    /// `row_height` is the resolved row box height the content is aligned within.
    #[must_use]
    pub fn content_y_offset(&self, row_height: Twip) -> Twip {
        let slack = (row_height.raw() - self.occupied_height().raw()).max(0);
        let valign = match self.vertical_alignment {
            CellVAlign::Top => 0,
            CellVAlign::Center => slack / 2,
            CellVAlign::Bottom => slack,
        };
        Twip(self.margins.top.raw() + valign)
    }
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
        /// Keep this row with the following table row because a conforming
        /// vertical merge crosses their boundary.
        #[serde(default, skip_serializing_if = "core::ops::Not::not")]
        merge_keep_next: bool,
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

    /// The tallest cell's stacked content height *including its top and bottom
    /// margins* (twips), independent of the row-height rule — used to reconcile
    /// `w:trHeight` against content. The margins are part of the row's minimum
    /// height: Word insets content by `w:tcMar` before the answer line, so a row
    /// with only top-margin content is still taller than its bare text.
    #[must_use]
    pub fn cells_content_height(cells: &[CellFragment]) -> Twip {
        cells
            .iter()
            .map(CellFragment::occupied_height)
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
            BlockFragment::TableRow {
                merge_keep_next, ..
            } => BreakControl {
                keep_next: *merge_keep_next,
                ..BreakControl::default()
            },
        }
    }

    /// Whether this row participates in a multi-row vertical-merge keep group.
    #[must_use]
    pub fn is_vertical_merge_row(&self) -> bool {
        matches!(
            self,
            BlockFragment::TableRow {
                merge_keep_next: true,
                ..
            }
        )
    }
}
