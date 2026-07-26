//! Hit-testing, caret geometry, and cursor navigation over a paginated layout.
//!
//! [`LayoutSnapshot`] is a read-only view over an immutable [`PaginatedLayout`]
//! that bridges *screen geometry* and *model positions* — the contract every
//! editor needs for caret placement, selection painting, mouse hit-testing, and
//! keyboard navigation. Following the LayoutNG discipline (`43-…` §3, §9), it
//! adds **no** layout cost: each method is a pure walk of the already-placed page
//! fragments. Everything is computed in twips (the device scale is applied later,
//! when a [`crate::display`] list is built).
//!
//! ## The layout↔model contract this module relies on
//!
//! - Each [`crate::text::Line`] carries a [`crate::model::ModelRange`]
//!   ([`Line::range`](crate::text::Line::range)) whose `start`/`end` offsets are
//!   the half-open span of the paragraph node's text that the line covers.
//! - Each [`crate::text::Glyph`] carries a `cluster` — the UTF-8 byte offset,
//!   *within the paragraph node's text*, of the cluster the glyph renders. This
//!   is the anchor that ties a painted glyph back to a caret position.
//! - Each [`crate::text::GlyphRun`] carries a `bidi_level`; its parity gives the
//!   run's direction (even = left-to-right, odd = right-to-left). Runs and the
//!   glyphs inside them are stored in visual (left-to-right) order, so mapping a
//!   click to a logical offset in an RTL run means reading the run backwards.
//!
//! From those three facts every position mapping below follows.

use crate::block::BlockFragment;
use crate::model::{ModelPos, ModelRange};
use crate::page::PaginatedLayout;
use crate::text::Line;
use crate::units::{Point, Rect, Size, Twip};

/// A direction for vertical caret movement (up/down arrow keys).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Toward the top of the document (previous visual line).
    Up,
    /// Toward the bottom of the document (next visual line).
    Down,
}

/// Whether a hit landed on painted content or was snapped in from outside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HitZone {
    /// The point fell within a line's box — a direct hit on content.
    Content,
    /// The point fell outside any line box (a page margin, or the leading/
    /// trailing whitespace of a line) and was snapped to the nearest caret
    /// position.
    Outside,
}

/// The outcome of resolving a page-local point to a model position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HitResult {
    /// The resolved model position (the nearest caret slot).
    pub pos: ModelPos,
    /// Whether the point was inside content or snapped in from outside it.
    pub zone: HitZone,
}

/// One caret slot within a line: a page-local x and the model offset a caret
/// placed there addresses.
#[derive(Clone, Copy, Debug)]
struct CaretStop {
    x: Twip,
    offset: u32,
}

/// A single laid-out line located in absolute page-local coordinates. This is the
/// flattened unit the snapshot walks; it is produced in flow order (pages in
/// order, fragments in order, lines top-to-bottom), which is exactly document
/// order — the order caret navigation and selection traverse.
struct LineBox<'a> {
    /// 1-based page number the line sits on.
    page: u32,
    /// Page-local x of the line's content origin (its fragment/cell left edge).
    left: Twip,
    /// Page-local y of the top of the line.
    top: Twip,
    /// The line itself.
    line: &'a Line,
}

impl<'a> LineBox<'a> {
    /// The y just past the bottom of the line.
    fn bottom(&self) -> i32 {
        self.top.raw() + self.line.height.raw()
    }

    /// Whether `y` (page-local) falls within this line's vertical band.
    fn contains_y(&self, y: i32) -> bool {
        y >= self.top.raw() && y < self.bottom()
    }
}

/// A read-only view over a [`PaginatedLayout`] that maps between screen geometry
/// and model positions.
///
/// It borrows the layout immutably and never mutates it — the same snapshot can
/// answer any number of caret/selection/navigation queries. Construct one per
/// paginated layout and drop it when the layout changes.
#[derive(Clone, Copy, Debug)]
pub struct LayoutSnapshot<'a> {
    layout: &'a PaginatedLayout,
}

impl<'a> LayoutSnapshot<'a> {
    /// Creates a snapshot over `layout`.
    #[must_use]
    pub fn new(layout: &'a PaginatedLayout) -> Self {
        Self { layout }
    }

    /// The layout this snapshot views.
    #[must_use]
    pub fn layout(&self) -> &'a PaginatedLayout {
        self.layout
    }

    /// Resolves a page-local `point` on page `page_number` (1-based) to the
    /// nearest model position.
    ///
    /// The walk is: find the line whose vertical band contains `point.y` (or the
    /// vertically nearest line if the click is above/below all content), then
    /// find the caret slot on that line whose x is nearest `point.x`. The
    /// returned [`HitResult::zone`] reports whether the point was a direct hit on
    /// content or was snapped in from a margin / a line's leading-or-trailing
    /// whitespace. Returns `None` only when the page has no content at all.
    #[must_use]
    pub fn hit_test(&self, page_number: u32, point: Point) -> Option<HitResult> {
        let lines = self.line_boxes();
        let page_lines: Vec<&LineBox<'_>> =
            lines.iter().filter(|lb| lb.page == page_number).collect();
        let first = *page_lines.first()?;

        // The line whose band contains the point, else the vertically nearest.
        let (lb, vertically_inside) =
            match page_lines.iter().find(|lb| lb.contains_y(point.y.raw())) {
                Some(lb) => (*lb, true),
                None => {
                    let nearest = page_lines
                        .iter()
                        .min_by_key(|lb| vertical_distance(lb, point.y.raw()))
                        .copied()
                        .unwrap_or(first);
                    (nearest, false)
                }
            };

        let stops = stops_for(lb.line, lb.left);
        let nearest = nearest_stop(&stops, point.x);
        let horizontally_inside =
            point.x.raw() >= line_left(&stops).raw() && point.x.raw() <= line_right(&stops).raw();
        let zone = if vertically_inside && horizontally_inside {
            HitZone::Content
        } else {
            HitZone::Outside
        };
        Some(HitResult {
            pos: ModelPos::new(lb.line.range.start.node, nearest.offset),
            zone,
        })
    }

    /// The caret box for `pos`: a zero-width, line-height-tall rectangle at the
    /// caret's position, plus the 1-based page it sits on. Returns `None` if no
    /// line covers `pos` (e.g. a stale position after an edit).
    ///
    /// At a soft-wrap boundary (an offset that ends one line and starts the next)
    /// the caret is placed at the *start of the following line*, the conventional
    /// affinity.
    #[must_use]
    pub fn caret_rect(&self, pos: ModelPos) -> Option<(u32, Rect)> {
        let lines = self.line_boxes();
        let idx = caret_start_line(&lines, pos)?;
        let lb = &lines[idx];
        let stops = stops_for(lb.line, lb.left);
        let x = stops
            .iter()
            .find(|s| s.offset == pos.offset)
            .map_or_else(|| nearest_stop(&stops, lb.left).x, |s| s.x);
        let rect = Rect::new(Point::new(x, lb.top), Size::new(Twip::ZERO, lb.line.height));
        Some((lb.page, rect))
    }

    /// One rectangle per line-fragment the `range` covers, in page-local twips,
    /// paired with the 1-based page each sits on.
    ///
    /// The first and last covered lines are clipped to the range's endpoints; the
    /// lines between are highlighted across their full inked width. The range may
    /// span paragraphs and page boundaries — the covered lines are simply the
    /// flow-order run from the line holding `range.start` to the line holding
    /// `range.end`. An empty or inverted range yields no rectangles.
    #[must_use]
    pub fn selection_rects(&self, range: ModelRange) -> Vec<(u32, Rect)> {
        let lines = self.line_boxes();
        let Some(mut start_i) = caret_start_line(&lines, range.start) else {
            return Vec::new();
        };
        let Some(mut end_i) = caret_end_line(&lines, range.end) else {
            return Vec::new();
        };
        if start_i > end_i {
            std::mem::swap(&mut start_i, &mut end_i);
        }

        let mut rects = Vec::new();
        for (i, lb) in lines.iter().enumerate().take(end_i + 1).skip(start_i) {
            let stops = stops_for(lb.line, lb.left);
            let left = line_left(&stops);
            let right = line_right(&stops);
            let at = |off: u32| {
                stops
                    .iter()
                    .find(|s| s.offset == off)
                    .map_or_else(|| nearest_stop(&stops, lb.left).x, |s| s.x)
            };
            let x0 = if i == start_i {
                at(range.start.offset)
            } else {
                left
            };
            let x1 = if i == end_i {
                at(range.end.offset)
            } else {
                right
            };
            let (lo, hi) = if x0.raw() <= x1.raw() {
                (x0, x1)
            } else {
                (x1, x0)
            };
            if lo.raw() == hi.raw() {
                continue;
            }
            rects.push((
                lb.page,
                Rect::new(Point::new(lo, lb.top), Size::new(hi - lo, lb.line.height)),
            ));
        }
        rects
    }

    /// Moves the caret at `pos` vertically to the adjacent visual line, returning
    /// the model position visually nearest the current caret x (its x-affinity).
    ///
    /// Navigation is in flow order, so the adjacent line may be on the previous or
    /// next page — moving down off the last line of a page lands on the first line
    /// of the next. Returns `None` at the document's top/bottom edge, or if `pos`
    /// itself does not resolve to a line.
    #[must_use]
    pub fn move_vertical(&self, pos: ModelPos, dir: Direction) -> Option<ModelPos> {
        let lines = self.line_boxes();
        let cur = caret_start_line(&lines, pos)?;
        let target = match dir {
            Direction::Up => cur.checked_sub(1)?,
            Direction::Down => {
                let next = cur + 1;
                (next < lines.len()).then_some(next)?
            }
        };

        // The x-affinity: the caret's current x on its own line.
        let cur_stops = stops_for(lines[cur].line, lines[cur].left);
        let affinity = cur_stops
            .iter()
            .find(|s| s.offset == pos.offset)
            .map_or_else(|| nearest_stop(&cur_stops, lines[cur].left).x, |s| s.x);

        let tgt = &lines[target];
        let stops = stops_for(tgt.line, tgt.left);
        let landed = nearest_stop(&stops, affinity);
        Some(ModelPos::new(tgt.line.range.start.node, landed.offset))
    }

    /// Flattens the layout into its lines, in flow (document) order, each located
    /// in absolute page-local coordinates.
    fn line_boxes(&self) -> Vec<LineBox<'a>> {
        let mut out = Vec::new();
        for page in &self.layout.pages {
            for placed in &page.placed {
                collect_fragment(
                    &placed.fragment,
                    placed.rect.origin.x,
                    placed.rect.origin.y,
                    page.number,
                    &mut out,
                );
            }
        }
        out
    }
}

/// Appends `fragment`'s lines (recursing into table cells) to `out`, located at
/// page-local (`left`, `top`).
fn collect_fragment<'a>(
    fragment: &'a BlockFragment,
    left: Twip,
    top: Twip,
    page: u32,
    out: &mut Vec<LineBox<'a>>,
) {
    match fragment {
        BlockFragment::Paragraph {
            lines, box_metrics, ..
        } => {
            let mut y = top + box_metrics.space_before;
            for line in &lines.lines {
                out.push(LineBox {
                    page,
                    left,
                    top: y,
                    line,
                });
                y = y + line.height;
            }
        }
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                let cell_left = left + cell.x;
                let mut cell_top = top;
                for block in &cell.blocks {
                    collect_fragment(block, cell_left, cell_top, page, out);
                    cell_top = cell_top + block.height();
                }
            }
        }
    }
}

/// The caret slots of a line, in visual order, in page-local x (`left` is the
/// line's content origin).
///
/// Each glyph contributes one slot at the boundary that opens its cluster; the
/// closing boundary of each run is added from the next distinct offset present on
/// the line (so ligatures and multi-run lines resolve without needing per-run
/// text lengths). For a right-to-left run the visual order is reversed, so the
/// slots are emitted at the glyphs' right edges and the run's logical end sits at
/// its visual-left edge.
fn stops_for(line: &Line, left: Twip) -> Vec<CaretStop> {
    // The distinct model offsets on the line, so a run's trailing boundary can be
    // resolved as "the next offset after the last cluster".
    let mut offsets = vec![line.range.start.offset, line.range.end.offset];
    for run in &line.runs {
        for glyph in &run.glyphs {
            offsets.push(glyph.cluster);
        }
    }
    offsets.sort_unstable();
    offsets.dedup();
    let next_after = |c: u32| -> u32 {
        offsets
            .iter()
            .copied()
            .find(|&o| o > c)
            .unwrap_or(line.range.end.offset)
    };

    let mut stops = Vec::new();
    for run in &line.runs {
        let base = left + run.origin.x;
        if run.bidi_level % 2 == 0 {
            // Left-to-right: a slot at each glyph's left edge, then the run's
            // trailing edge.
            let mut x = base;
            for glyph in &run.glyphs {
                stops.push(CaretStop {
                    x,
                    offset: glyph.cluster,
                });
                x = x + glyph.advance;
            }
            if let Some(last) = run.glyphs.last() {
                stops.push(CaretStop {
                    x,
                    offset: next_after(last.cluster),
                });
            }
        } else {
            // Right-to-left: a slot at each glyph's right edge (its cluster is the
            // logically-leading side), and the run's logical end at its left edge.
            let mut x = base;
            for glyph in &run.glyphs {
                x = x + glyph.advance;
                stops.push(CaretStop {
                    x,
                    offset: glyph.cluster,
                });
            }
            if let Some(first) = run.glyphs.first() {
                stops.push(CaretStop {
                    x: base,
                    offset: next_after(first.cluster),
                });
            }
        }
    }

    if stops.is_empty() {
        // An empty line (e.g. an empty paragraph) still has one caret slot.
        stops.push(CaretStop {
            x: left,
            offset: line.range.start.offset,
        });
    }
    stops
}

/// The caret slot whose x is nearest `x` (ties resolve to the first).
fn nearest_stop(stops: &[CaretStop], x: Twip) -> CaretStop {
    stops
        .iter()
        .copied()
        .min_by_key(|s| (s.x.raw() - x.raw()).abs())
        .unwrap_or(CaretStop { x, offset: 0 })
}

/// The x of the leftmost caret slot.
fn line_left(stops: &[CaretStop]) -> Twip {
    stops.iter().map(|s| s.x).min().unwrap_or(Twip::ZERO)
}

/// The x of the rightmost caret slot.
fn line_right(stops: &[CaretStop]) -> Twip {
    stops.iter().map(|s| s.x).max().unwrap_or(Twip::ZERO)
}

/// The vertical distance from `y` to a line's band (0 if inside).
fn vertical_distance(lb: &LineBox<'_>, y: i32) -> i32 {
    if y < lb.top.raw() {
        lb.top.raw() - y
    } else if y >= lb.bottom() {
        y - lb.bottom() + 1
    } else {
        0
    }
}

/// The flow index of the line that owns `pos` when placing a caret *at the start*
/// of a boundary: the line strictly covering `pos`, else (for the document/
/// paragraph end) the line that ends exactly at `pos`.
fn caret_start_line(lines: &[LineBox<'_>], pos: ModelPos) -> Option<usize> {
    let mut end_match = None;
    for (i, lb) in lines.iter().enumerate() {
        if lb.line.range.start.node != pos.node {
            continue;
        }
        let start = lb.line.range.start.offset;
        let end = lb.line.range.end.offset;
        if start <= pos.offset && pos.offset < end {
            return Some(i);
        }
        if pos.offset == end {
            end_match = Some(i);
        }
    }
    end_match
}

/// The flow index of the line that owns `pos` when placing a caret *at the end*
/// of a boundary: the line that ends exactly at `pos` (so a selection stops at the
/// visual end of that line rather than the empty start of the next), else the line
/// strictly covering `pos`.
fn caret_end_line(lines: &[LineBox<'_>], pos: ModelPos) -> Option<usize> {
    let mut cover = None;
    for (i, lb) in lines.iter().enumerate() {
        if lb.line.range.start.node != pos.node {
            continue;
        }
        let start = lb.line.range.start.offset;
        let end = lb.line.range.end.offset;
        if pos.offset == end {
            return Some(i);
        }
        if cover.is_none() && start <= pos.offset && pos.offset < end {
            cover = Some(i);
        }
    }
    cover
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockFragment, BoxMetrics, BreakControl};
    use crate::page::PaginatedLayout;
    use crate::paginate::{PageConfig, paginate};
    use crate::text::{Decoration, FontId, Glyph, GlyphRun, Line, LineBreak, LineLayout};
    use crate::units::Size;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::SectionId;

    const ADV: i32 = 100;
    const LINE_H: i32 = 240;
    const MARGIN: i32 = 1_440;

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    fn letter_config() -> PageConfig {
        PageConfig {
            section: SectionId::new(node(9)),
            page_size: Size::new(Twip(12_240), Twip(15_840)),
            margin_top: Twip(MARGIN),
            margin_bottom: Twip(MARGIN),
            margin_start: Twip(MARGIN),
            margin_end: Twip(MARGIN),
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        }
    }

    /// An LTR paragraph whose lines hold `line_lens[i]` single-byte "glyphs",
    /// offsets running consecutively across the whole paragraph.
    fn ltr_para(id: u64, line_lens: &[u32]) -> BlockFragment {
        let n = node(id);
        let total: u32 = line_lens.iter().sum();
        let mut off = 0u32;
        let mut baseline = LINE_H;
        let mut lines = Vec::new();
        for (li, &len) in line_lens.iter().enumerate() {
            let start = off;
            let glyphs = (0..len)
                .map(|i| Glyph {
                    id: 1,
                    advance: Twip(ADV),
                    cluster: start + i,
                })
                .collect();
            let run = GlyphRun {
                font: FontId(0),
                size: Twip(LINE_H),
                color: [0, 0, 0, 255],
                origin: Point::new(Twip::ZERO, Twip(baseline)),
                bidi_level: 0,
                decoration: Decoration::default(),
                highlight: None,
                glyphs,
            };
            off += len;
            lines.push(Line {
                runs: vec![run],
                ascent: Twip(LINE_H),
                descent: Twip::ZERO,
                height: Twip(LINE_H),
                range: ModelRange::new(ModelPos::new(n, start), ModelPos::new(n, off)),
                line_break: if li + 1 == line_lens.len() {
                    LineBreak::ParagraphEnd
                } else {
                    LineBreak::Wrap
                },
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
            });
            baseline += LINE_H;
        }
        let _ = total;
        BlockFragment::Paragraph {
            id: n,
            lines: LineLayout { lines },
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: crate::block::ParagraphDecor::default(),
        }
    }

    /// A single-line RTL paragraph of `n` single-byte glyphs. Glyphs are stored in
    /// visual (left-to-right) order, so clusters run high→low.
    fn rtl_para(id: u64, n: u32) -> BlockFragment {
        let node = node(id);
        let glyphs = (0..n)
            .map(|visual| Glyph {
                id: 1,
                advance: Twip(ADV),
                cluster: n - 1 - visual,
            })
            .collect();
        let run = GlyphRun {
            font: FontId(0),
            size: Twip(LINE_H),
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip(LINE_H)),
            bidi_level: 1,
            decoration: Decoration::default(),
            highlight: None,
            glyphs,
        };
        let line = Line {
            runs: vec![run],
            ascent: Twip(LINE_H),
            descent: Twip::ZERO,
            height: Twip(LINE_H),
            range: ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, n)),
            line_break: LineBreak::ParagraphEnd,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines: vec![line] },
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: crate::block::ParagraphDecor::default(),
        }
    }

    fn layout(fragments: &[BlockFragment]) -> PaginatedLayout {
        paginate(fragments, &letter_config())
    }

    #[test]
    fn caret_and_hit_test_round_trip() {
        let frags = vec![ltr_para(1, &[3, 3, 3])];
        let paginated = layout(&frags);
        let snap = LayoutSnapshot::new(&paginated);
        let n = node(1);

        for off in [0u32, 1, 3, 4, 6, 9] {
            let pos = ModelPos::new(n, off);
            let (page, rect) = snap.caret_rect(pos).expect("a caret for every offset");
            assert_eq!(rect.size.width, Twip::ZERO, "caret is zero-width");
            assert_eq!(rect.size.height, Twip(LINE_H), "caret is line-height tall");
            let center = Point::new(rect.origin.x, Twip(rect.origin.y.raw() + LINE_H / 2));
            let hit = snap.hit_test(page, center).expect("a hit inside content");
            assert_eq!(hit.pos, pos, "caret→hit round-trips for offset {off}");
            assert_eq!(hit.zone, HitZone::Content);
        }
    }

    #[test]
    fn clicking_past_a_lines_end_snaps_to_the_line_end() {
        let frags = vec![ltr_para(1, &[3, 3, 3])];
        let paginated = layout(&frags);
        let snap = LayoutSnapshot::new(&paginated);
        // Well to the right of line 0 (offsets 0..3), within its vertical band.
        let far_right = Point::new(Twip(MARGIN + 10_000), Twip(MARGIN + LINE_H / 2));
        let hit = snap.hit_test(1, far_right).expect("still resolves");
        assert_eq!(hit.pos, ModelPos::new(node(1), 3), "snaps to the line end");
        assert_eq!(
            hit.zone,
            HitZone::Outside,
            "past the ink is outside content"
        );
    }

    #[test]
    fn selection_over_multiple_lines_yields_one_box_per_line() {
        let frags = vec![ltr_para(1, &[3, 3, 3])];
        let paginated = layout(&frags);
        let snap = LayoutSnapshot::new(&paginated);
        let n = node(1);
        // Offset 1 (line 0) → offset 8 (line 2): three covered lines.
        let range = ModelRange::new(ModelPos::new(n, 1), ModelPos::new(n, 8));
        let rects = snap.selection_rects(range);
        assert_eq!(rects.len(), 3, "one box per covered line");
        // All on page 1, each a line tall, tops strictly descending the page.
        for (page, _) in &rects {
            assert_eq!(*page, 1);
        }
        for (_, rect) in &rects {
            assert_eq!(rect.size.height, Twip(LINE_H));
        }
        assert!(rects[0].1.origin.y.raw() < rects[1].1.origin.y.raw());
        assert!(rects[1].1.origin.y.raw() < rects[2].1.origin.y.raw());
        // First box starts at the caret for offset 1 (one glyph in from the left).
        assert_eq!(rects[0].1.origin.x, Twip(MARGIN + ADV));
        // First box runs to the line's right edge (three glyphs wide).
        assert_eq!(rects[0].1.right(), Twip(MARGIN + 3 * ADV));
        // Middle box spans the full inked width.
        assert_eq!(rects[1].1.origin.x, Twip(MARGIN));
        assert_eq!(rects[1].1.right(), Twip(MARGIN + 3 * ADV));
        // Last box ends at the caret for offset 8 (two glyphs in on line 2).
        assert_eq!(rects[2].1.origin.x, Twip(MARGIN));
        assert_eq!(rects[2].1.right(), Twip(MARGIN + 2 * ADV));
    }

    #[test]
    fn rtl_run_resolves_left_and_right_clicks_to_logical_ends() {
        let frags = vec![rtl_para(1, 3)];
        let paginated = layout(&frags);
        let snap = LayoutSnapshot::new(&paginated);
        let n = node(1);
        let y = Twip(MARGIN + LINE_H / 2);

        // Visual-left edge of an RTL run is its logical END (offset 3).
        let left_click = Point::new(Twip(MARGIN + 5), y);
        assert_eq!(
            snap.hit_test(1, left_click).unwrap().pos,
            ModelPos::new(n, 3),
            "a click at the visual left of an RTL run is the logical end"
        );
        // Visual-right edge is its logical START (offset 0).
        let right_click = Point::new(Twip(MARGIN + 3 * ADV - 5), y);
        assert_eq!(
            snap.hit_test(1, right_click).unwrap().pos,
            ModelPos::new(n, 0),
            "a click at the visual right of an RTL run is the logical start"
        );
        // A caret at offset 3 (logical end) paints at the visual-left edge.
        let (_, rect) = snap.caret_rect(ModelPos::new(n, 3)).unwrap();
        assert_eq!(rect.origin.x, Twip(MARGIN));
    }

    #[test]
    fn vertical_navigation_preserves_the_x_column() {
        let frags = vec![ltr_para(1, &[3, 3, 3])];
        let paginated = layout(&frags);
        let snap = LayoutSnapshot::new(&paginated);
        let n = node(1);
        // On line 1, one glyph in (offset 4, x = MARGIN + ADV).
        let start = ModelPos::new(n, 4);
        let up = snap.move_vertical(start, Direction::Up).unwrap();
        assert_eq!(up, ModelPos::new(n, 1), "up keeps the same visual column");
        let down = snap.move_vertical(start, Direction::Down).unwrap();
        assert_eq!(
            down,
            ModelPos::new(n, 7),
            "down keeps the same visual column"
        );
        // Off the top of the document there is nowhere to go.
        assert!(
            snap.move_vertical(ModelPos::new(n, 0), Direction::Up)
                .is_none()
        );
    }

    #[test]
    fn vertical_navigation_crosses_a_page_boundary() {
        // 55 single-line paragraphs of 240 twips: the 12_960-twip content area
        // holds 54, so paragraph 55 opens page 2.
        let frags: Vec<_> = (1..=55).map(|i| ltr_para(i, &[1])).collect();
        let paginated = layout(&frags);
        assert_eq!(paginated.page_count(), 2, "content spans two pages");
        let snap = LayoutSnapshot::new(&paginated);

        // The last line of page 1 is paragraph 54.
        let last_on_page1 = ModelPos::new(node(54), 0);
        assert_eq!(snap.caret_rect(last_on_page1).unwrap().0, 1);

        let down = snap
            .move_vertical(last_on_page1, Direction::Down)
            .expect("there is a line below");
        assert_eq!(
            down.node,
            node(55),
            "moving down lands on page 2's paragraph"
        );
        assert_eq!(
            snap.caret_rect(down).unwrap().0,
            2,
            "the landed caret is on page 2"
        );
    }

    #[test]
    fn caret_rect_is_none_for_an_unknown_node() {
        let frags = vec![ltr_para(1, &[3])];
        let paginated = layout(&frags);
        let snap = LayoutSnapshot::new(&paginated);
        assert!(snap.caret_rect(ModelPos::new(node(999), 0)).is_none());
    }

    #[test]
    fn hit_test_on_an_empty_page_is_none() {
        let paginated = PaginatedLayout::default();
        let snap = LayoutSnapshot::new(&paginated);
        assert!(snap.hit_test(1, Point::default()).is_none());
    }

    #[test]
    fn works_over_a_real_shaped_galley() {
        // A galley shaped by the production ParleyShaper, paginated, then
        // hit-tested — the module must not panic on real shaper output.
        use crate::shape::ParleyShaper;
        use crate::text::{LineConstraints, LineShaper, StyledRun};

        let n = node(1);
        let range = ModelRange::new(ModelPos::new(n, 0), ModelPos::new(n, 11));
        let shaper = ParleyShaper::new();
        let run = StyledRun {
            text: "Hello world".into(),
            font: FontId(0),
            size: Twip::from_points(11),
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
            highlight: None,
            baseline_shift: Twip::ZERO,
        };
        let shaped = shaper.shape_paragraph(&[run], LineConstraints::default(), range);
        let frag = BlockFragment::Paragraph {
            id: n,
            lines: shaped,
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: crate::block::ParagraphDecor::default(),
        };
        let paginated = layout(&[frag]);
        let snap = LayoutSnapshot::new(&paginated);

        let (page, rect) = snap.caret_rect(ModelPos::new(n, 0)).expect("a caret");
        let center = Point::new(rect.origin.x, Twip(rect.origin.y.raw() + LINE_H / 2));
        assert!(
            snap.hit_test(page, center).is_some(),
            "resolves a real galley"
        );
    }

    #[test]
    fn real_shaper_caret_round_trips_to_the_correct_byte() {
        // The end-to-end contract the shaper now satisfies: with byte-accurate
        // clusters and per-line ranges from `ParleyShaper`, a caret placed at a
        // model byte offset paints at a position that hit-tests back to the *same*
        // byte. "Hello world" is ASCII, so each character is its own cluster and
        // byte offsets 0..=11 are all addressable caret slots.
        use crate::shape::ParleyShaper;
        use crate::text::{LineConstraints, LineShaper, StyledRun};

        let n = node(1);
        let text = "Hello world";
        let range = ModelRange::new(ModelPos::new(n, 0), ModelPos::new(n, text.len() as u32));
        let shaper = ParleyShaper::new();
        let run = StyledRun {
            text: text.into(),
            font: FontId(0),
            size: Twip::from_points(11),
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
            highlight: None,
            baseline_shift: Twip::ZERO,
        };
        // A wide column keeps it on one line so every offset is present.
        let shaped = shaper.shape_paragraph(
            &[run],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            range,
        );
        assert_eq!(shaped.lines.len(), 1, "the text fits on one line");
        // The line's range spans the whole ASCII string.
        assert_eq!(shaped.lines[0].range.start.offset, 0);
        assert_eq!(shaped.lines[0].range.end.offset, text.len() as u32);

        let frag = BlockFragment::Paragraph {
            id: n,
            lines: shaped,
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: crate::block::ParagraphDecor::default(),
        };
        let paginated = layout(&[frag]);
        let snap = LayoutSnapshot::new(&paginated);

        for off in [0u32, 1, 5, 6, 10, 11] {
            let pos = ModelPos::new(n, off);
            let (page, rect) = snap
                .caret_rect(pos)
                .unwrap_or_else(|| panic!("a caret for byte {off}"));
            let center = Point::new(rect.origin.x, Twip(rect.origin.y.raw() + LINE_H / 2));
            let hit = snap
                .hit_test(page, center)
                .unwrap_or_else(|| panic!("a hit for byte {off}"));
            assert_eq!(hit.pos, pos, "caret→hit round-trips at byte {off}");
        }
    }
}
