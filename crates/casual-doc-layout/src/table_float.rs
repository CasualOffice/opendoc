//! Positioned (floating) tables — `w:tblPr/w:tblpPr` (`CT_TblPPr`,
//! ECMA-376 §17.4.58), the table half of the float layer.
//!
//! A table carrying `w:tblpPr` is **not** a block in the flow. Word lifts it out,
//! positions it against a page/margin/text reference frame exactly as it
//! positions a `wp:anchor` drawing, and wraps the surrounding body text around
//! it at the four `*FromText` distances. Until this module existed the property
//! round-tripped through the model and import/export (`P1F-29`) with **zero**
//! layout consumers, so every positioned table rendered as an ordinary inline
//! block: in the wrong place, and with the text that should sit beside it pushed
//! below (`docs/105` FID-L-07, `docs/109` row 64).
//!
//! # One mechanism, not two
//!
//! A positioned table is the same *kind* of object as a floating drawing, so it
//! reuses the drawing float machinery rather than growing a parallel rule:
//!
//! - **Position** resolves through [`crate::anchor::resolve_body_float_rect`],
//!   which is the same [`resolve_anchor_rect`](crate::anchor) the drawing pass
//!   uses. [`table_float_anchor`] is the only new code: it maps the `w:tblpPr`
//!   vocabulary onto the `wp:anchor` vocabulary.
//! - **Wrapping** resolves through the existing [`BodyWrapRect`] →
//!   `paragraph_float_exclusions` → `FlowItem::FloatExclusion` path, so a
//!   positioned table narrows lines through the same bounded fixed point in
//!   [`crate::document_layout`] that a page-relative picture does.
//! - **Painting and hit-testing** go through [`AnchorContent::Table`], carried on
//!   the page's float layer beside every other float.
//!
//! This mirrors ONLYOFFICE (AGPL — read for behaviour, nothing copied):
//! `CDocumentContent.Recalculate_Page` branches on `Element.IsTable() &&
//! !Element.IsInline()` and hands the recalculated table to the *drawing* layer
//! as `DrawingObjects.addFloatTable(new CFlowTable(Element, PageIndex))`; a
//! `CFlowTable` (`word/Editor/FlowObjects.js`) is simply a square-wrap flow
//! object whose rect is the table's own page bounds and whose `Distance` is the
//! four from-text values. Their `CTableAnchorPosition.Calculate_X/Calculate_Y`
//! (`word/Editor/Table.js`) is the behaviour [`table_float_anchor`] reproduces.
//!
//! # Complexity
//!
//! [`flow_positions`] walks every page's placed fragments **once** per pass and
//! builds a block-id → placement index, so resolving *n* positioned tables is
//! `O(pages × placed + n·log n)`, not `O(n × pages × placed)`. Flowing a
//! positioned table is `O(that table)` and happens once per pagination pass.
//!
//! # Deliberate limits of this slice (recorded, not hidden)
//!
//! - **Top-level body tables only.** A positioned table nested in a cell, an
//!   `w:sdt`, a header/footer, or a note body keeps today's inline behaviour.
//! - **A positioned table does not split across pages.** It is carried as one
//!   [`PlacedAnchor`], which is a single-page object. Word breaks a tall
//!   positioned table; we clamp it to the page its anchor resolved on.
//! - **`w:tblOverlap="never"` is honoured only between positioned tables**
//!   (ONLYOFFICE's `Correct_Values` rule: push the later table down), not
//!   against floating drawings.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, DrawingAnchor, Extent, SectionId, Table, TableAnchor, TableFloatPosition,
    TableOverlap, TableXAlign, TableYAlign, WrapDistances, WrapMode,
};
// Kept on separate `use` lines (anti-conflict, per the parallel-import rule).
use casual_doc_model::v1::{AnchorHorizontal, AnchorVertical};
use casual_doc_model::v1::{HorizontalAlign, HorizontalAnchor, HorizontalPosition};
use casual_doc_model::v1::{VerticalAlign, VerticalAnchor, VerticalPosition};

use crate::anchor::{BodyWrapRect, body_section_ids, resolve_body_float_rect};
use crate::block::BlockFragment;
use crate::page::{AnchorContent, AnchorZ, PaginatedLayout, PlacedAnchor};
use crate::paginate::PageConfig;
use crate::text::LineShaper;
use crate::units::{Point, Rect, Size, Twip};

/// 635 EMU per twip (914 400 EMU per inch ÷ 1 440 twips per inch).
const EMU_PER_TWIP: i64 = 635;

/// The ECMA-376 `ST_TwipsMeasure` ceiling import already clamps `w:tblpPr`
/// offsets and from-text distances to (`22` inches). Re-applied here so a
/// hand-built [`Document`] cannot drive the resolver outside twip range.
const MAX_TBLP_TWIPS: i32 = 31_680;

/// One positioned table discovered in the body, with everything the placement
/// pass needs about it.
#[derive(Clone, Copy)]
pub(crate) struct FloatingTable<'a> {
    /// The table node.
    pub(crate) table: &'a Table,
    /// Its `w:tblpPr`.
    pub(crate) position: &'a TableFloatPosition,
    /// The section whose page geometry its anchors resolve against.
    pub(crate) section: SectionId,
    /// Its index in `document.body()`.
    pub(crate) body_index: usize,
}

/// Every **top-level body** table carrying `w:tblpPr`, in document order.
///
/// `O(body)`. Nested tables are deliberately excluded — see the module's
/// "deliberate limits".
#[must_use]
pub(crate) fn floating_tables<'a>(
    document: &'a Document,
    fallback_section: SectionId,
) -> Vec<FloatingTable<'a>> {
    let sections = body_section_ids(document, fallback_section);
    document
        .body()
        .iter()
        .zip(sections)
        .enumerate()
        .filter_map(|(body_index, (block, section))| {
            let BlockNode::Table(table) = block else {
                return None;
            };
            let position = table.properties.float_position.as_ref()?;
            Some(FloatingTable {
                table,
                position,
                section,
                body_index,
            })
        })
        .collect()
}

/// The ids of every top-level body table that floats. `O(body)`.
#[must_use]
pub(crate) fn floating_table_ids(document: &Document) -> BTreeSet<NodeId> {
    document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Table(table) if table.properties.float_position.is_some() => Some(table.id),
            _ => None,
        })
        .collect()
}

/// Whether the document positions any table. The cheap guard every caller uses
/// before doing float work. `O(body)`.
#[must_use]
pub(crate) fn has_floating_table(document: &Document) -> bool {
    document.body().iter().any(|block| {
        matches!(block, BlockNode::Table(table) if table.properties.float_position.is_some())
    })
}

/// Drops a positioned table's rows out of a freshly built body galley.
///
/// This is what takes the table **out of block flow**: the paginator never sees
/// its rows, so the text that follows it flows into the space it would have
/// occupied and the wrap exclusions computed by [`wrap_rects`] are what push
/// that text aside. Without this the table would reserve a full-width band and
/// "wrapping" would be decorative.
///
/// COMPLEXITY. `O(body)` for the check, then `O(galley)` for the retain — and
/// **nothing is allocated** unless the document actually positions a table.
/// This also runs on the incremental (per-keystroke) galley path, where the
/// budget is `O(1)` in document size (`docs/107` §4). It does not change that
/// path's complexity class: `build_galley_cached_labeled` already opens with an
/// `O(body)` `contains_drop_cap_pair` scan, so this adds a constant factor to a
/// walk that was already there rather than a new order of growth.
pub(crate) fn lift_floating_rows(galley: &mut Vec<BlockFragment>, document: &Document) {
    if !has_floating_table(document) {
        return;
    }
    let floating = floating_table_ids(document);
    galley.retain(|fragment| match fragment {
        BlockFragment::TableRow { table, .. } => !floating.contains(table),
        BlockFragment::Paragraph { .. } => true,
    });
}

/// A positioned table resolved against a paginated layout: which page it landed
/// on, its page-local rectangle, and its flowed rows.
pub(crate) struct ResolvedFloatingTable {
    /// The table node (so a click can resolve back to it).
    pub(crate) id: NodeId,
    /// The page it was placed on.
    pub(crate) page_index: usize,
    /// Its page-local rectangle.
    pub(crate) rect: Rect,
    /// The four text-wrap distances, as the drawing float layer expresses them.
    pub(crate) distances: WrapDistances,
    /// Its flowed rows, positioned relative to `rect.origin`.
    pub(crate) rows: Vec<BlockFragment>,
}

/// Resolves every positioned body table against an already paginated layout.
///
/// Shared by the wrap pass and the placement pass so both see byte-identical
/// geometry — the same reason the drawing float layer resolves its rect twice
/// from one function rather than caching it across passes.
///
/// `O(pages × placed + tables × table)`.
#[must_use]
pub(crate) fn resolve(
    layout: &PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
) -> Vec<ResolvedFloatingTable> {
    // The cheap guard first: `floating_tables` allocates a section id per body
    // block, and this runs once per wrap pass per fixed-point iteration, so a
    // document with no positioned table must not pay for that allocation.
    if layout.pages.is_empty() || !has_floating_table(document) {
        return Vec::new();
    }
    let tables = floating_tables(document, config.section);
    if tables.is_empty() {
        return Vec::new();
    }
    let positions = flow_positions(layout, document);
    let mut out: Vec<ResolvedFloatingTable> = Vec::with_capacity(tables.len());
    for floating in tables {
        let (page_index, anchor_box, column) =
            anchor_frame(layout, document, &positions, floating.body_index);
        let rows = flow_rows(document, shaper, floating.body_index, column.size.width);
        let size = rows_size(&rows);
        let anchor = table_float_anchor(floating.position);
        let mut rect = resolve_body_float_rect(
            document,
            config,
            floating.section,
            anchor_box,
            column,
            &anchor,
            extent_of(size),
        );
        // A positioned table is one single-page object in this slice, so keep it
        // on the page its anchor resolved on rather than letting a tall table
        // paint off the sheet (module docs, "deliberate limits").
        let content = layout.pages[page_index].content_area;
        rect = clamp_to_page(rect, content, layout.pages[page_index].page_size);
        // `w:tblOverlap="never"`: Word — and ONLYOFFICE's
        // `CTableAnchorPosition.Correct_Values` — pushes the later table DOWN
        // clear of an earlier one it would cover, never sideways.
        if floating.table.properties.overlap != Some(TableOverlap::Overlap) {
            rect = displace_from_earlier(rect, page_index, &out);
        }
        out.push(ResolvedFloatingTable {
            id: floating.table.id,
            page_index,
            rect,
            distances: anchor.wrap_distances,
            rows,
        });
    }
    out
}

/// The wrap rectangles positioned tables contribute to the document driver's
/// exclusion fixed point, keyed by the table's own node id (which
/// `paragraph_float_exclusions` admits alongside top-level paragraphs).
#[must_use]
pub(crate) fn wrap_rects(
    layout: &PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
) -> Vec<BodyWrapRect> {
    resolve(layout, document, shaper, config)
        .into_iter()
        .map(|table| BodyWrapRect {
            page_index: table.page_index,
            source: table.id,
            rect: table.rect,
            distances: table.distances,
        })
        .collect()
}

/// Places every positioned body table onto the float layer of the page its
/// anchor resolved on — the table counterpart of
/// [`place_floats`](crate::anchor::place_floats), and run immediately after it
/// so the two share one document-order z-space.
pub(crate) fn place_floating_tables(
    layout: &mut PaginatedLayout,
    document: &Document,
    shaper: &dyn LineShaper,
    config: &PageConfig,
) {
    let resolved = resolve(layout, document, shaper, config);
    for (index, table) in resolved.into_iter().enumerate() {
        let order = u32::try_from(index).unwrap_or(u32::MAX);
        layout.pages[table.page_index].anchored.push(PlacedAnchor {
            node: Some(table.id),
            content: AnchorContent::Table { rows: table.rows },
            rect: table.rect,
            // A positioned table always paints over the body text; Word has no
            // `behindDoc` for `w:tblpPr`.
            behind_doc: false,
            z: AnchorZ {
                // `w:tblpPr` carries no `relativeHeight`, so positioned tables
                // share one band and order among themselves by document order —
                // above every `relativeHeight`-0 drawing, which is what Word
                // shows for a table dropped over a picture.
                relative_height: 0,
                order,
            },
            descr: None,
            transform: None,
        });
    }
}

// --- `w:tblpPr` → `wp:anchor` ---------------------------------------------

/// Maps a `w:tblpPr` onto the `wp:anchor` vocabulary the drawing float layer
/// already resolves, so a positioned table and a positioned picture obey one
/// placement rule.
///
/// The mapping, and why each arm is what it is:
///
/// | `w:tblpPr` | `wp:anchor` | Why |
/// | --- | --- | --- |
/// | `horzAnchor="text"` (and absent) | `relativeFrom="column"` | `ST_HAnchor` `text` is the *text column*, and `text` is Word's default when the attribute is omitted. ONLYOFFICE collapses `text` onto `margin` because it has no columns; we have them, so the column frame is used. |
/// | `horzAnchor="margin"` / `"page"` | `margin` / `page` | Same frames. |
/// | `vertAnchor="text"` (and absent) | `relativeFrom="paragraph"` | Both mean "the current position in the flow". |
/// | `vertAnchor="margin"` / `"page"` | `margin` / `page` | Same frames. |
/// | `tblpXSpec` / `tblpYSpec` | `<wp:align>` | The named form wins over the offset (§17.4.58), which is exactly `HorizontalPosition::Align`'s precedence over `Offset`. |
/// | `tblpX` / `tblpY` (twips) | `<wp:posOffset>` (EMU) | ×635, exact. |
/// | `tblpYSpec="inline"` | `align="top"` | `ST_YAlign` `inline` has no `wp:align` counterpart; Word (and ONLYOFFICE's `Calculate_Y`) treats it as top-of-frame. |
/// | the four `*FromText` | `distT`/`distB`/`distL`/`distR` | Same quantity, twips → EMU. |
/// | — | `wrap="square"` | A positioned table always wraps text; `w:tblpPr` has no wrap-mode attribute. |
#[must_use]
fn table_float_anchor(position: &TableFloatPosition) -> DrawingAnchor {
    let horizontal = AnchorHorizontal {
        relative_from: match position.horz_anchor.unwrap_or(TableAnchor::Text) {
            TableAnchor::Text => HorizontalAnchor::Column,
            TableAnchor::Margin => HorizontalAnchor::Margin,
            TableAnchor::Page => HorizontalAnchor::Page,
        },
        position: match position.x_spec {
            Some(spec) => HorizontalPosition::Align(match spec {
                TableXAlign::Left => HorizontalAlign::Left,
                TableXAlign::Center => HorizontalAlign::Center,
                TableXAlign::Right => HorizontalAlign::Right,
                TableXAlign::Inside => HorizontalAlign::Inside,
                TableXAlign::Outside => HorizontalAlign::Outside,
            }),
            None => HorizontalPosition::Offset(twips_to_emu(position.tbl_px_twips.unwrap_or(0))),
        },
    };
    let vertical = AnchorVertical {
        relative_from: match position.vert_anchor.unwrap_or(TableAnchor::Text) {
            TableAnchor::Text => VerticalAnchor::Paragraph,
            TableAnchor::Margin => VerticalAnchor::Margin,
            TableAnchor::Page => VerticalAnchor::Page,
        },
        position: match position.y_spec {
            Some(spec) => VerticalPosition::Align(match spec {
                // `ST_YAlign` `inline` has no `wp:align` counterpart: the table
                // sits where the flow left off, which for the paragraph frame is
                // its top edge.
                TableYAlign::Inline | TableYAlign::Top => VerticalAlign::Top,
                TableYAlign::Center => VerticalAlign::Center,
                TableYAlign::Bottom => VerticalAlign::Bottom,
                TableYAlign::Inside => VerticalAlign::Inside,
                TableYAlign::Outside => VerticalAlign::Outside,
            }),
            None => VerticalPosition::Offset(twips_to_emu(position.tbl_py_twips.unwrap_or(0))),
        },
    };
    DrawingAnchor {
        horizontal,
        vertical,
        // `w:tblpPr` has no wrap-mode attribute: a positioned table always wraps
        // text around its rectangle.
        wrap: WrapMode::Square,
        wrap_distances: WrapDistances {
            top_emu: twips_to_emu(position.top_from_text_twips.unwrap_or(0)),
            bottom_emu: twips_to_emu(position.bottom_from_text_twips.unwrap_or(0)),
            start_emu: twips_to_emu(position.left_from_text_twips.unwrap_or(0)),
            end_emu: twips_to_emu(position.right_from_text_twips.unwrap_or(0)),
        },
        wrap_polygon: None,
        behind_doc: false,
    }
}

// --- Geometry helpers ------------------------------------------------------

/// Where each top-level body block landed: its page and its page-local box.
///
/// Built in **one** walk of the layout so resolving many positioned tables never
/// re-walks the pages (the `paragraph_properties`-in-a-loop trap, `SKILL` §8).
/// A block spanning pages keeps its FIRST page and the union of its boxes there,
/// which is the flow position a following block resolves against.
fn flow_positions(
    layout: &PaginatedLayout,
    document: &Document,
) -> BTreeMap<NodeId, (usize, Rect)> {
    let top_level: BTreeSet<NodeId> = document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id),
            BlockNode::Table(table) => Some(table.id),
            BlockNode::Sdt(_) | BlockNode::AltChunk(_) => None,
        })
        .collect();
    let mut out: BTreeMap<NodeId, (usize, Rect)> = BTreeMap::new();
    for (page_index, page) in layout.pages.iter().enumerate() {
        for placed in &page.placed {
            let id = match &placed.fragment {
                BlockFragment::Paragraph { id, .. } => *id,
                BlockFragment::TableRow { table, .. } => *table,
            };
            if !top_level.contains(&id) {
                continue;
            }
            match out.get_mut(&id) {
                Some((existing_page, rect)) if *existing_page == page_index => {
                    *rect = union(*rect, placed.rect);
                }
                Some(_) => {}
                None => {
                    out.insert(id, (page_index, placed.rect));
                }
            }
        }
    }
    out
}

/// The page, text-flow anchor box, and text column a positioned table at
/// `body_index` resolves against.
///
/// Its rows were lifted out of the galley, so the block that FOLLOWS it now
/// flows into the slot it would have occupied: that following block's placed
/// top is the flow position, and it is correct across a page break for free. A
/// table with nothing placed after it falls back to the bottom of the nearest
/// preceding placed block, then to the top of the first page's content area.
fn anchor_frame(
    layout: &PaginatedLayout,
    document: &Document,
    positions: &BTreeMap<NodeId, (usize, Rect)>,
    body_index: usize,
) -> (usize, Rect, Rect) {
    let body = document.body();
    let mut found = None;
    for block in body.iter().skip(body_index.saturating_add(1)) {
        if let Some(placed) = block_id(block).and_then(|id| positions.get(&id)) {
            found = Some((placed.0, placed.1.origin.y, placed.1));
            break;
        }
    }
    if found.is_none() {
        for block in body.iter().take(body_index).rev() {
            if let Some(placed) = block_id(block).and_then(|id| positions.get(&id)) {
                found = Some((placed.0, placed.1.bottom(), placed.1));
                break;
            }
        }
    }
    let (page_index, flow_y, neighbour) = match found {
        Some(found) => found,
        None => {
            let content = layout.pages[0].content_area;
            (0, content.origin.y, content)
        }
    };
    let content = layout.pages[page_index].content_area;
    // The text column is the neighbouring block's own x/width (so a positioned
    // table in a narrower column resolves against THAT column), spanning the
    // page's content height.
    let column = Rect::new(
        Point::new(neighbour.origin.x, content.origin.y),
        Size::new(neighbour.size.width, content.size.height),
    );
    // A `vertAnchor="text"` offset is measured from the flow position, so the
    // anchor box is the zero-height line at that position.
    let anchor_box = Rect::new(
        Point::new(column.origin.x, flow_y),
        Size::new(column.size.width, Twip::ZERO),
    );
    (page_index, anchor_box, column)
}

/// The id a top-level body block is placed under.
fn block_id(block: &BlockNode) -> Option<NodeId> {
    match block {
        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
        BlockNode::Table(table) => Some(table.id),
        BlockNode::Sdt(_) | BlockNode::AltChunk(_) => None,
    }
}

/// Flows one positioned table through the **same** block pipeline the body uses,
/// at the width of its own text column (`SKILL`'s uniform-flow rule: no
/// context-limited feature set for a table just because it floats).
fn flow_rows(
    document: &Document,
    shaper: &dyn LineShaper,
    body_index: usize,
    width: Twip,
) -> Vec<BlockFragment> {
    let body = document.body();
    let Some(block) = body.get(body_index) else {
        return Vec::new();
    };
    crate::flow::build_galley_for_blocks_inner(
        document,
        shaper,
        core::slice::from_ref(block),
        width.max(Twip(1)),
        None,
        // The float layer flows its content in the editing byte space, exactly
        // as `flow_anchored_text_box` does for a floating text box; the
        // post-pagination passes are not threaded a `ReviewView`.
        crate::flow::ReviewView::Editing,
        crate::flow::NoteFlow::default(),
        None,
    )
}

/// The size a flowed table occupies: the widest row's trailing cell edge by the
/// stacked height of every row.
fn rows_size(rows: &[BlockFragment]) -> Size {
    let mut width = Twip::ZERO;
    let mut height = Twip::ZERO;
    for row in rows {
        height = height + row.height();
        if let BlockFragment::TableRow { cells, .. } = row {
            for cell in cells {
                let right = cell.x + cell.width;
                if right.raw() > width.raw() {
                    width = right;
                }
            }
        }
    }
    Size::new(width.max(Twip(1)), height.max(Twip(1)))
}

/// Keeps a positioned table inside the sheet it was placed on. A table taller
/// than the content area is pinned to the content top rather than being pushed
/// off the bottom edge.
fn clamp_to_page(rect: Rect, content: Rect, page: Size) -> Rect {
    let max_x = (page.width.raw() - rect.size.width.raw()).max(0);
    let max_y = (content.bottom().raw() - rect.size.height.raw()).max(content.origin.y.raw());
    Rect::new(
        Point::new(
            Twip(rect.origin.x.raw().clamp(0, max_x)),
            Twip(rect.origin.y.raw().min(max_y).max(0)),
        ),
        rect.size,
    )
}

/// `w:tblOverlap="never"`: push this table down clear of every earlier
/// positioned table on the same page it would otherwise cover.
///
/// Word — and ONLYOFFICE's `CTableAnchorPosition.Correct_Values` — displaces
/// **vertically only**; a horizontal shift changes the table's own line breaking
/// and so cannot be applied after the fact.
fn displace_from_earlier(rect: Rect, page_index: usize, placed: &[ResolvedFloatingTable]) -> Rect {
    let mut rect = rect;
    let mut moved = true;
    // Each iteration clears at least one earlier table, and a cleared table is
    // never re-entered (they are only ever pushed further down), so this
    // terminates in at most `placed.len()` rounds.
    let mut rounds = 0usize;
    while moved && rounds <= placed.len() {
        moved = false;
        rounds = rounds.saturating_add(1);
        for other in placed.iter().filter(|o| o.page_index == page_index) {
            let intersects = rect.origin.x.raw() < other.rect.right().raw()
                && rect.right().raw() > other.rect.origin.x.raw()
                && rect.origin.y.raw() < other.rect.bottom().raw()
                && rect.bottom().raw() > other.rect.origin.y.raw();
            if intersects {
                rect = Rect::new(Point::new(rect.origin.x, other.rect.bottom()), rect.size);
                moved = true;
            }
        }
    }
    rect
}

/// The union of two page-local boxes.
fn union(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.raw().min(b.origin.x.raw());
    let top = a.origin.y.raw().min(b.origin.y.raw());
    let right = a.right().raw().max(b.right().raw());
    let bottom = a.bottom().raw().max(b.bottom().raw());
    Rect::new(
        Point::new(Twip(left), Twip(top)),
        Size::new(Twip(right - left), Twip(bottom - top)),
    )
}

/// A twip size as the EMU [`Extent`] the shared anchor resolver takes.
fn extent_of(size: Size) -> Extent {
    Extent {
        width_emu: i64::from(size.width.raw()) * EMU_PER_TWIP,
        height_emu: i64::from(size.height.raw()) * EMU_PER_TWIP,
    }
}

/// A `w:tblpPr` twip measure as EMU, clamped to the schema's own range so a
/// hand-built model cannot overflow the resolver.
fn twips_to_emu(twips: i32) -> i64 {
    i64::from(twips.clamp(-MAX_TBLP_TWIPS, MAX_TBLP_TWIPS)) * EMU_PER_TWIP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position() -> TableFloatPosition {
        TableFloatPosition::default()
    }

    #[test]
    fn an_absent_anchor_defaults_to_the_text_frame_like_word() {
        let anchor = table_float_anchor(&position());
        assert_eq!(anchor.horizontal.relative_from, HorizontalAnchor::Column);
        assert_eq!(anchor.vertical.relative_from, VerticalAnchor::Paragraph);
        assert_eq!(anchor.horizontal.position, HorizontalPosition::Offset(0));
        assert_eq!(anchor.vertical.position, VerticalPosition::Offset(0));
        assert_eq!(anchor.wrap, WrapMode::Square);
    }

    #[test]
    fn a_named_alignment_wins_over_an_absolute_offset() {
        let anchor = table_float_anchor(&TableFloatPosition {
            tbl_px_twips: Some(1_440),
            x_spec: Some(TableXAlign::Center),
            tbl_py_twips: Some(720),
            y_spec: Some(TableYAlign::Bottom),
            ..position()
        });
        assert_eq!(
            anchor.horizontal.position,
            HorizontalPosition::Align(HorizontalAlign::Center)
        );
        assert_eq!(
            anchor.vertical.position,
            VerticalPosition::Align(VerticalAlign::Bottom)
        );
    }

    #[test]
    fn offsets_and_from_text_distances_convert_to_emu_exactly() {
        let anchor = table_float_anchor(&TableFloatPosition {
            horz_anchor: Some(TableAnchor::Page),
            vert_anchor: Some(TableAnchor::Margin),
            tbl_px_twips: Some(1_440),
            tbl_py_twips: Some(-720),
            left_from_text_twips: Some(180),
            right_from_text_twips: Some(187),
            top_from_text_twips: Some(90),
            bottom_from_text_twips: Some(72),
            ..position()
        });
        assert_eq!(anchor.horizontal.relative_from, HorizontalAnchor::Page);
        assert_eq!(anchor.vertical.relative_from, VerticalAnchor::Margin);
        assert_eq!(
            anchor.horizontal.position,
            HorizontalPosition::Offset(914_400)
        );
        assert_eq!(anchor.vertical.position, VerticalPosition::Offset(-457_200));
        assert_eq!(anchor.wrap_distances.start_emu, 114_300);
        assert_eq!(anchor.wrap_distances.end_emu, 118_745);
        assert_eq!(anchor.wrap_distances.top_emu, 57_150);
        assert_eq!(anchor.wrap_distances.bottom_emu, 45_720);
    }

    #[test]
    fn an_inline_y_spec_resolves_to_the_top_of_its_frame() {
        let anchor = table_float_anchor(&TableFloatPosition {
            y_spec: Some(TableYAlign::Inline),
            ..position()
        });
        assert_eq!(
            anchor.vertical.position,
            VerticalPosition::Align(VerticalAlign::Top)
        );
    }

    #[test]
    fn an_out_of_range_offset_clamps_to_the_schema_ceiling() {
        let anchor = table_float_anchor(&TableFloatPosition {
            tbl_px_twips: Some(i32::MAX),
            ..position()
        });
        assert_eq!(
            anchor.horizontal.position,
            HorizontalPosition::Offset(i64::from(MAX_TBLP_TWIPS) * EMU_PER_TWIP)
        );
    }
}
