//! Per-table horizontal scrolling in a reflowed column — the engine half of
//! Google Docs' pageless wide-table behaviour (`docs/151` §6.3d).
//!
//! # The problem
//!
//! Under [`MeasureFit::Scroll`](crate::flow::MeasureFit::Scroll) a top-level body
//! table keeps the width its document declares, so in a reflowed column it can
//! be wider than the tile it is placed on. A tile's raster is exactly the column
//! plus its two gutters, so the part past the right edge is not painted. Google's
//! pageless answer — *"you can create wide tables and view them by scrolling left
//! and right"* — is a scroller per table. The host owns that scroller (a native
//! scroll container over the table's band); what it needs from the engine is the
//! table's **geometry** and a **horizontal offset** that every geometry consumer
//! honours, so a click, the caret, a selection and the raster all agree about
//! which column is under the reader's finger.
//!
//! # The established pattern, and why the offset is applied to the layout
//!
//! This is a scroll container — a viewport onto content wider than itself, with
//! the content translated by the scroll offset. The textbook implementation for a
//! retained scene is to translate the subtree's geometry, not to teach every
//! reader of it about an offset: the wasm facade has 36 call sites that read
//! the painted layout (render, hit test, caret, selection, cell rects, table
//! chrome …), and an offset applied at read time is an offset one of them
//! forgets. So the
//! offset is applied ONCE, to the placed rows themselves, and every consumer is
//! correct by construction.
//!
//! **It is idempotent, and that is the property the design turns on.** In a
//! reflow layout every placed body fragment sits at the content area's left edge
//! — a reflowed tile is a one-column section, the column paginator places at
//! `column.x + x_shift`, and `x_shift` is zero because the tile's gutters are
//! symmetric — so a scrolled row's offset is not state that can go stale: it is
//! `content_area.x - rect.x`, readable off the page itself. Setting
//! an offset therefore writes an ABSOLUTE position (`content_area.x - offset`),
//! applying the same offset twice changes nothing, and resetting is writing zero.
//! There is no "applied" ledger to desynchronise from the layout it describes.
//! `every_placed_body_fragment_sits_at_the_content_left_edge` in
//! `tests/reflow.rs` guards the invariant this rests on.
//!
//! # Cost
//!
//! A scroll gesture is an interaction, so it must be `O(1)` in document size
//! (`docs/107` §4). [`scroll_table`] walks only the contiguous run of tiles the
//! table occupies, outward from a tile the host names — `O(rows of that table)`,
//! never the document. Re-applying every remembered offset after a relayout
//! ([`apply_table_scroll`]) is one pass over the placed fragments and is only run
//! when some table has been scrolled; a relayout in reflow is already a walk over
//! every tile (`docs/151` §4.4a), so it changes no complexity class.

use std::collections::BTreeMap;

use casual_doc_model::NodeId;

use crate::block::{BlockFragment, CellFragment};
use crate::page::{Page, PaginatedLayout};
use crate::units::{Point, Rect, Size, Twip};

/// One over-wide table's band on one tile, in that tile's own twips.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableOverflow {
    /// The table node.
    pub table: NodeId,
    /// The top of the table's first row on this tile.
    pub top: Twip,
    /// The height of the table's rows on this tile.
    pub height: Twip,
    /// The width of the scrolled content: from the tile's left edge to the
    /// table's right edge plus the tile's own right gutter, so the table's last
    /// column comes to rest against the same gutter its first one starts at. The
    /// widest row of the table across ALL its tiles decides it, so every strip of
    /// one table scrolls the same distance.
    pub scroll_width: Twip,
    /// The visible width — the tile's own width.
    pub viewport: Twip,
    /// The offset currently applied, `0..=scroll_width - viewport`.
    pub offset: Twip,
}

impl TableOverflow {
    /// The furthest the table can be scrolled.
    #[must_use]
    pub fn max_offset(&self) -> Twip {
        Twip((self.scroll_width.raw() - self.viewport.raw()).max(0))
    }
}

/// The right edge of a row's cells, relative to the row's placed origin —
/// including a separated-cell gap, which is painted.
fn row_extent(cells: &[CellFragment]) -> i32 {
    cells
        .iter()
        .map(|cell| cell.x.raw() + cell.width.raw() + cell.cell_spacing.end.raw())
        .max()
        .unwrap_or(0)
}

/// The table a placed fragment is a row of, and that row's extent.
fn table_row(fragment: &BlockFragment) -> Option<(NodeId, i32)> {
    match fragment {
        BlockFragment::TableRow { table, cells, .. } => Some((*table, row_extent(cells))),
        BlockFragment::Paragraph { .. } => None,
    }
}

/// Whether `page` holds a row of `table`, and the widest such row's extent.
fn extent_on(page: &Page, table: NodeId) -> Option<i32> {
    page.placed
        .iter()
        .filter_map(|placed| table_row(&placed.fragment))
        .filter(|(id, _)| *id == table)
        .map(|(_, extent)| extent)
        .max()
}

/// The contiguous run of tiles `table` occupies around `hint`, and its widest
/// row. A table's rows are consecutive in flow, so they occupy consecutive
/// tiles: walking outward until a tile without one is the whole table.
///
/// Complexity: `O(rows of the table)` plus the fragments on the tiles it shares.
fn table_span(layout: &PaginatedLayout, hint: usize, table: NodeId) -> Option<(usize, usize, i32)> {
    let mut widest = extent_on(layout.pages.get(hint)?, table)?;
    let mut first = hint;
    while first > 0 {
        let Some(extent) = extent_on(&layout.pages[first - 1], table) else {
            break;
        };
        widest = widest.max(extent);
        first -= 1;
    }
    let mut last = hint;
    while last + 1 < layout.pages.len() {
        let Some(extent) = extent_on(&layout.pages[last + 1], table) else {
            break;
        };
        widest = widest.max(extent);
        last += 1;
    }
    Some((first, last, widest))
}

/// The scroll width a table of `extent` has on `page`: the tile's left gutter,
/// the table, and the tile's right gutter.
fn scroll_width(page: &Page, extent: i32) -> i32 {
    let left = page.content_area.origin.x.raw();
    let right_gutter =
        (page.page_size.width.raw() - left - page.content_area.size.width.raw()).max(0);
    left + extent + right_gutter
}

/// Writes `offset` onto every row of `table` on `page`: an ABSOLUTE position,
/// `content_area.x - offset`, so applying it twice changes nothing.
fn place_rows(page: &mut Page, table: NodeId, offset: i32) {
    let x = Twip(page.content_area.origin.x.raw() - offset);
    for placed in &mut page.placed {
        if matches!(&placed.fragment, BlockFragment::TableRow { table: id, .. } if *id == table) {
            placed.rect.origin.x = x;
        }
    }
}

/// The over-wide tables on tile `index`, top to bottom.
///
/// A table is over-wide when its widest row reaches past the tile's own raster —
/// past the right GUTTER, not merely past the column: a table that overhangs the
/// column by a border's width is fully painted, and a scroller for a few twips
/// would be a control for nothing.
///
/// Complexity: `O(fragments on the tile)`, plus `O(rows)` of each over-wide table
/// to find its widest row across tiles.
#[must_use]
pub fn table_overflows(layout: &PaginatedLayout, index: usize) -> Vec<TableOverflow> {
    let Some(page) = layout.pages.get(index) else {
        return Vec::new();
    };
    let viewport = page.page_size.width.raw();
    let content_x = page.content_area.origin.x.raw();
    // Each table's band on this tile, in first-seen (top-to-bottom) order.
    let mut bands: Vec<(NodeId, i32, i32, i32)> = Vec::new();
    for placed in &page.placed {
        let Some((table, extent)) = table_row(&placed.fragment) else {
            continue;
        };
        let top = placed.rect.origin.y.raw();
        let bottom = top + placed.fragment.height().raw();
        match bands.iter_mut().find(|band| band.0 == table) {
            Some(band) => {
                band.1 = band.1.min(top);
                band.2 = band.2.max(bottom);
                band.3 = band.3.max(extent);
            }
            None => bands.push((table, top, bottom, extent)),
        }
    }
    bands
        .into_iter()
        .filter(|&(_, _, _, extent)| content_x + extent > viewport)
        .map(|(table, top, bottom, _)| {
            let widest = table_span(layout, index, table).map_or(0, |(_, _, widest)| widest);
            let offset = page
                .placed
                .iter()
                .find(|placed| matches!(table_row(&placed.fragment), Some((id, _)) if id == table))
                .map_or(0, |placed| content_x - placed.rect.origin.x.raw());
            TableOverflow {
                table,
                top: Twip(top),
                height: Twip(bottom - top),
                scroll_width: Twip(scroll_width(page, widest)),
                viewport: Twip(viewport),
                offset: Twip(offset.max(0)),
            }
        })
        .collect()
}

/// Scrolls `table` to `offset`, on every tile it occupies, and returns the
/// offset actually applied — clamped to `0..=max`. `hint` is any tile the table
/// is on; the host always has one, because it is scrolling a strip that sits on
/// a tile. `None` when the table is not on `hint` or does not overflow.
///
/// Complexity: `O(rows of the table)` — never the document (`docs/107` §4).
pub fn scroll_table(
    layout: &mut PaginatedLayout,
    hint: usize,
    table: NodeId,
    offset: Twip,
) -> Option<Twip> {
    let (first, last, widest) = table_span(layout, hint, table)?;
    let page = &layout.pages[hint];
    let max = scroll_width(page, widest) - page.page_size.width.raw();
    if max <= 0 {
        return None;
    }
    let applied = offset.raw().clamp(0, max);
    for page in &mut layout.pages[first..=last] {
        place_rows(page, table, applied);
    }
    Some(Twip(applied))
}

/// Writes every remembered offset in `offsets` onto a freshly built reflow
/// layout — or, with `reset`, writes zero for each of them, which is how a host
/// hands a layout back to the incremental paginator exactly as it was built.
///
/// Each offset is clamped to its table's current scroll range, because the
/// relayout that called this may have changed the table (an edit) or the
/// column (a width step), and an offset past the end would scroll the table off
/// the raster entirely. A table no longer in the document is simply not found.
///
/// Idempotent (see the module comment). Complexity: `O(placed fragments)` — run
/// only after a relayout and only while some table has been scrolled.
pub fn apply_table_scroll(
    layout: &mut PaginatedLayout,
    offsets: &BTreeMap<NodeId, Twip>,
    reset: bool,
) {
    if offsets.is_empty() {
        return;
    }
    // Pass 1: each remembered table's widest row and the tile geometry, so the
    // clamp is per TABLE and every tile of it agrees.
    let mut range: BTreeMap<NodeId, i32> = BTreeMap::new();
    for page in &layout.pages {
        for placed in &page.placed {
            let Some((table, extent)) = table_row(&placed.fragment) else {
                continue;
            };
            if !offsets.contains_key(&table) {
                continue;
            }
            let max = scroll_width(page, extent) - page.page_size.width.raw();
            let entry = range.entry(table).or_insert(i32::MIN);
            *entry = (*entry).max(max);
        }
    }
    // Pass 2: write the clamped offsets.
    for page in &mut layout.pages {
        let tables: Vec<NodeId> = page
            .placed
            .iter()
            .filter_map(|placed| table_row(&placed.fragment).map(|(id, _)| id))
            .filter(|id| range.contains_key(id))
            .collect();
        for table in tables {
            let max = range.get(&table).map_or(0, |max| (*max).max(0));
            let wanted = if reset {
                0
            } else {
                offsets.get(&table).map_or(0, |offset| offset.raw())
            };
            place_rows(page, table, wanted.clamp(0, max));
        }
    }
}

/// A page holding only `table`'s rows from tile `index`, laid out at offset ZERO
/// and moved up so the band starts at `y = 0`, sized to the full scroll width —
/// what the host rasterises once into its scroll strip and then scrolls natively,
/// so a scroll gesture costs the compositor and not the engine.
///
/// Everything that is not one of the table's rows is cleared: running content,
/// floats, notes, borders, line numbers and the watermark belong to the tile, and
/// the tile is still drawn beneath the strip.
///
/// Complexity: `O(fragments on the tile)` to filter, then the clone of the
/// table's own rows.
#[must_use]
pub fn table_strip_page(layout: &PaginatedLayout, index: usize, table: NodeId) -> Option<Page> {
    let overflow = table_overflows(layout, index)
        .into_iter()
        .find(|overflow| overflow.table == table)?;
    let page = &layout.pages[index];
    let content_x = page.content_area.origin.x;
    let mut strip = page.clone();
    strip.clear_post_pagination();
    strip.separators.clear();
    strip.placed.retain(|placed| {
        matches!(&placed.fragment, BlockFragment::TableRow { table: id, .. } if *id == table)
    });
    for placed in &mut strip.placed {
        placed.rect.origin = Point::new(content_x, placed.rect.origin.y - overflow.top);
    }
    strip.page_size = Size::new(overflow.scroll_width, overflow.height);
    strip.content_area = Rect::new(
        Point::new(content_x, Twip::ZERO),
        Size::new(
            Twip((overflow.scroll_width.raw() - 2 * content_x.raw()).max(0)),
            overflow.height,
        ),
    );
    Some(strip)
}
