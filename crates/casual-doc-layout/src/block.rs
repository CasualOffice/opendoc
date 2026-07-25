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

/// One cell's laid-out content within a table-row fragment.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CellFragment {
    /// The cell node.
    pub id: NodeId,
    /// Column span (`w:gridSpan`).
    pub grid_span: u32,
    /// The cell's flowed block fragments.
    pub blocks: Vec<BlockFragment>,
}

/// A block fragment in flow order. A paragraph carries its shaped lines; a table
/// row carries its cells and its split-control flags (the paginator honors
/// `can_split`/`header` when a row crosses a page boundary).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BlockFragment {
    /// A paragraph: its shaped lines plus box metrics.
    Paragraph {
        /// The paragraph node.
        id: NodeId,
        /// Shaped lines.
        lines: LineLayout,
        /// Surrounding box space.
        box_metrics: BoxMetrics,
    },
    /// A table row: cells, whether it may split across a page, and whether it is
    /// a repeated header row (`w:tblHeader`).
    TableRow {
        /// The row node.
        id: NodeId,
        /// The row's cells.
        cells: Vec<CellFragment>,
        /// May the row split across a page boundary? (`false` = `w:cantSplit`.)
        can_split: bool,
        /// Is this a header row repeated at the top of each page?
        header: bool,
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
            BlockFragment::TableRow { cells, .. } => cells
                .iter()
                .map(|cell| {
                    cell.blocks
                        .iter()
                        .map(BlockFragment::height)
                        .fold(Twip::ZERO, |a, h| a + h)
                })
                .max()
                .unwrap_or(Twip::ZERO),
        }
    }

    /// The document node this fragment came from (its anchor for pagination
    /// boundaries and hit-testing).
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        match self {
            BlockFragment::Paragraph { id, .. } | BlockFragment::TableRow { id, .. } => *id,
        }
    }
}
