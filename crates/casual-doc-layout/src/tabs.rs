//! DOCX tab-stop resolution and hard-break line assembly, layered over the base
//! [`LineShaper`].
//!
//! The base shaper ([`crate::shape`]) shapes and wraps a run of uniform text but
//! knows nothing about DOCX tab stops or hard breaks (its trait contract says so).
//! This module is the caller-side layer that:
//!
//! - splits a paragraph's inline stream at hard breaks (`w:br`/`w:cr`) into
//!   forced lines, threading page/column breaks to the paginator; and
//! - resolves explicit (`w:tabs`) and default (`w:defaultTabStop`) tab stops into
//!   real horizontal advances, positioning the text after each tab by the stop's
//!   alignment (left/center/right/decimal), drawing leaders (dot/hyphen/…) in the
//!   gap, and drawing `bar` stops as vertical rules.
//!
//! A paragraph with neither a tab, a break, nor a `bar` stop takes the fast path
//! (the base shaper alone), so ordinary text is byte-for-byte unchanged.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BreakKind, TabAlignment, TabLeader, TabStop};

use crate::model::{ModelPos, ModelRange};
use crate::text::{
    Decoration, Glyph, GlyphRun, Line, LineBreak, LineConstraints, LineLayout, LineShaper,
    StyledRun,
};
use crate::units::{Point, Twip};

/// Word's standard default tab-stop interval (720 twips = 0.5in), used when the
/// document declares no `w:defaultTabStop`.
pub const DEFAULT_TAB_STOP: Twip = Twip(720);

/// A resolved default tab interval from the document settings, or the standard
/// fallback. `settings` is `w:defaultTabStop` in twips.
#[must_use]
pub fn default_tab_stop(setting: Option<i32>) -> Twip {
    match setting {
        Some(v) if v > 0 => Twip(v),
        _ => DEFAULT_TAB_STOP,
    }
}

/// One item in a paragraph's flattened inline stream: a styled text run, an
/// explicit tab, or a hard break. Wrapper inlines (hyperlinks, revisions, content
/// controls) are already flattened into runs by the collector.
#[derive(Debug)]
pub enum FlowItem<'a> {
    /// A styled text run.
    Run(StyledRun<'a>),
    /// An explicit tab (`w:tab`) — advances to the next tab stop.
    Tab,
    /// A hard break (`w:br`/`w:cr`) with its kind.
    Break(BreakKind),
}

/// Whether an item stream needs the tab/break layer at all: any tab, any hard
/// break, or any `bar` tab stop (which draws even without a tab character). When
/// this is `false`, the caller uses the base shaper directly.
#[must_use]
pub fn needs_flow_layout(items: &[FlowItem<'_>], tab_stops: &[TabStop]) -> bool {
    items
        .iter()
        .any(|i| matches!(i, FlowItem::Tab | FlowItem::Break(_)))
        || tab_stops.iter().any(|t| t.alignment == TabAlignment::Bar)
}

/// A single text segment (the run of runs between two tabs / a break / line ends),
/// measured by shaping it unwrapped with the base shaper.
struct Segment {
    /// Shaped glyph runs, origins relative to the segment's own left edge (x from
    /// 0) at its baseline.
    runs: Vec<GlyphRun>,
    /// The segment's total advance width.
    width: Twip,
    /// Ascent above the baseline.
    ascent: Twip,
    /// Descent below the baseline.
    descent: Twip,
    /// The x of the decimal separator (`.`) within the segment, if any (for a
    /// decimal-aligned tab stop).
    decimal_x: Option<Twip>,
}

impl Segment {
    /// An empty segment (no text between two adjacent tabs, or a trailing tab).
    fn empty() -> Self {
        Self {
            runs: Vec::new(),
            width: Twip::ZERO,
            ascent: Twip::ZERO,
            descent: Twip::ZERO,
            decimal_x: None,
        }
    }
}

/// Shapes and lays out a paragraph's `items` into lines, resolving tab stops and
/// hard breaks. `tab_stops` are the paragraph's explicit `w:tabs`; `default_tab`
/// is the resolved default interval; `constraints` carry the wrap width and
/// paragraph alignment for the (tab-free) blocks; `range` anchors line offsets to
/// the paragraph node (its `start.offset` is the byte base, normally 0).
#[must_use]
pub fn shape_with_flow(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    let node = range.start.node;
    // Bar stops draw a vertical rule on every line of the paragraph.
    let bars: Vec<Twip> = tab_stops
        .iter()
        .filter(|t| t.alignment == TabAlignment::Bar)
        .map(|t| Twip(t.position_twips))
        .collect();

    let blocks = split_blocks(items, range.start.offset);
    let mut out: Vec<Line> = Vec::new();
    let mut cursor_y = Twip::ZERO;
    let last_block = blocks.len().saturating_sub(1);

    for (bi, block) in blocks.iter().enumerate() {
        let is_first = bi == 0;
        let is_last = bi == last_block;
        // The break that ends this block: `Hard` line-break metadata, and a
        // page/column break threaded to the paginator.
        let trailing = block.trailing;
        let first_line_indent = if is_first {
            constraints.first_line_indent
        } else {
            Twip::ZERO
        };

        let mut lines = if block.has_tab {
            layout_tabbed_line(
                shaper,
                node,
                block,
                tab_stops,
                default_tab,
                first_line_indent,
            )
        } else {
            layout_wrapped_block(shaper, node, block, constraints, first_line_indent)
        };

        // Stack this block's lines below the ones already emitted.
        for line in &mut lines {
            for run in &mut line.runs {
                run.origin.y = run.origin.y + cursor_y;
            }
            line.bars = bars.clone();
        }
        // Mark how the block ends on its last line.
        if let Some(last) = lines.last_mut() {
            match trailing {
                Some(kind) => {
                    last.line_break = LineBreak::Hard;
                    last.page_break_after = matches!(kind, BreakKind::Page | BreakKind::Column);
                }
                None if is_last => last.line_break = LineBreak::ParagraphEnd,
                None => last.line_break = LineBreak::Hard,
            }
        }
        cursor_y = cursor_y
            + lines
                .iter()
                .map(|l| l.height)
                .fold(Twip::ZERO, |a, h| a + h);
        out.extend(lines);
    }

    // An all-empty paragraph (e.g. a lone break) still needs a line so it has
    // height; fall back to a single empty line anchored at the node.
    if out.is_empty() {
        out.push(empty_line(node, range.start.offset, bars));
    }
    LineLayout { lines: out }
}

/// A block of items between hard breaks: its runs/tabs (with byte offsets) and the
/// break kind that terminates it (if any).
struct Block<'a> {
    /// Segments of styled runs split at tabs; `segments.len() == tabs + 1`.
    segments: Vec<Vec<(&'a StyledRun<'a>, u32)>>,
    /// The tab-stop leaders/alignment are resolved at layout time, so a tab is
    /// just a boundary here; this records whether the block contains any.
    has_tab: bool,
    /// Node byte offset at which the block starts (for line ranges).
    start_offset: u32,
    /// Node byte offset at which the block ends.
    end_offset: u32,
    /// The break that ends this block.
    trailing: Option<BreakKind>,
}

/// Splits the item stream into hard-break-delimited [`Block`]s, assigning each run
/// its node-relative byte offset (offsets accumulate over run text only; tabs and
/// breaks are zero-width for offset purposes, matching the paragraph text being
/// the concatenation of its runs).
fn split_blocks<'a>(items: &'a [FlowItem<'a>], base: u32) -> Vec<Block<'a>> {
    let mut blocks = Vec::new();
    let mut byte = base;
    let mut start_offset = base;
    let mut segments: Vec<Vec<(&StyledRun<'_>, u32)>> = vec![Vec::new()];
    let mut has_tab = false;

    for item in items {
        match item {
            FlowItem::Run(run) => {
                segments
                    .last_mut()
                    .expect("segments is never empty")
                    .push((run, byte));
                byte += run.text.len() as u32;
            }
            FlowItem::Tab => {
                has_tab = true;
                segments.push(Vec::new());
            }
            FlowItem::Break(kind) => {
                blocks.push(Block {
                    segments: std::mem::replace(&mut segments, vec![Vec::new()]),
                    has_tab,
                    start_offset,
                    end_offset: byte,
                    trailing: Some(*kind),
                });
                has_tab = false;
                start_offset = byte;
            }
        }
    }
    blocks.push(Block {
        segments,
        has_tab,
        start_offset,
        end_offset: byte,
        trailing: None,
    });
    blocks
}

/// Lays out a tab-free block via the base shaper (full wrapping, paragraph
/// alignment). Returns lines with `line_break`/`page_break_after` left at their
/// shaper defaults; the caller stamps the block-boundary break.
fn layout_wrapped_block(
    shaper: &dyn LineShaper,
    node: NodeId,
    block: &Block<'_>,
    constraints: LineConstraints,
    first_line_indent: Twip,
) -> Vec<Line> {
    let runs: Vec<StyledRun<'_>> = block
        .segments
        .iter()
        .flat_map(|seg| seg.iter().map(|(run, _)| (*run).clone()))
        .collect();
    let block_range = ModelRange::new(
        ModelPos::new(node, block.start_offset),
        ModelPos::new(node, block.end_offset),
    );
    let block_constraints = LineConstraints {
        first_line_indent,
        ..constraints
    };
    let layout = shaper.shape_paragraph(&runs, block_constraints, block_range);
    if layout.lines.is_empty() {
        return vec![empty_line(node, block.start_offset, Vec::new())];
    }
    layout.lines
}

/// Lays out a block that contains tabs onto a single line, resolving each tab to a
/// horizontal advance and positioning the following segment by the stop's
/// alignment, filling leaders in the gap. (A tabbed line is not soft-wrapped:
/// tab-positioned columns are a single line in Word's common uses — TOC rows,
/// forms, aligned columns.)
fn layout_tabbed_line(
    shaper: &dyn LineShaper,
    node: NodeId,
    block: &Block<'_>,
    tab_stops: &[TabStop],
    default_tab: Twip,
    first_line_indent: Twip,
) -> Vec<Line> {
    // Measure every segment unwrapped.
    let segments: Vec<Segment> = block
        .segments
        .iter()
        .map(|seg| measure_segment(shaper, node, seg))
        .collect();

    // The line's vertical metrics are the max over all its segments.
    let ascent = segments
        .iter()
        .map(|s| s.ascent)
        .max()
        .unwrap_or(Twip::ZERO);
    let descent = segments
        .iter()
        .map(|s| s.descent)
        .max()
        .unwrap_or(Twip::ZERO);
    let baseline = ascent;

    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut pen = first_line_indent.raw().max(0);

    for (i, seg) in segments.iter().enumerate() {
        if i == 0 {
            // The leading segment is left-aligned at the start (no preceding tab).
            push_shifted(&mut runs, &seg.runs, Twip(pen), baseline);
            pen += seg.width.raw();
            continue;
        }
        // A tab precedes segment `i`: resolve the stop it advances to.
        let stop = resolve_next_stop(pen, tab_stops, default_tab);
        // Where the following segment's box is placed relative to the stop.
        let mut left = match stop.alignment {
            TabAlignment::Start | TabAlignment::Bar => stop.position,
            TabAlignment::End => stop.position - seg.width.raw(),
            TabAlignment::Center => stop.position - seg.width.raw() / 2,
            TabAlignment::Decimal => stop.position - seg.decimal_x.unwrap_or(seg.width).raw(),
        };
        // Never overlap the preceding content.
        if left < pen {
            left = pen;
        }
        // Draw the leader (if any) across the gap the tab jumps.
        if let Some(leader) = stop.leader
            && left > pen
            && let Some(run) = leader_run(shaper, block, i, leader, Twip(pen), Twip(left), baseline)
        {
            runs.push(run);
        }
        push_shifted(&mut runs, &seg.runs, Twip(left), baseline);
        pen = left + seg.width.raw();
    }

    let line = Line {
        runs,
        ascent,
        descent,
        height: ascent + descent,
        range: ModelRange::new(
            ModelPos::new(node, block.start_offset),
            ModelPos::new(node, block.end_offset),
        ),
        line_break: LineBreak::ParagraphEnd,
        page_break_after: false,
        bars: Vec::new(),
    };
    vec![line]
}

/// Shifts a segment's shaped runs by `dx` horizontally and onto `baseline`,
/// appending them to `out`.
fn push_shifted(out: &mut Vec<GlyphRun>, runs: &[GlyphRun], dx: Twip, baseline: Twip) {
    for run in runs {
        let mut placed = run.clone();
        placed.origin = Point::new(run.origin.x + dx, baseline);
        out.push(placed);
    }
}

/// Measures one segment (a slice of styled runs) by shaping it unwrapped, and
/// records the x of its decimal separator for decimal-aligned tabs.
fn measure_segment(
    shaper: &dyn LineShaper,
    node: NodeId,
    seg: &[(&StyledRun<'_>, u32)],
) -> Segment {
    if seg.is_empty() {
        return Segment::empty();
    }
    let base = seg[0].1;
    let runs: Vec<StyledRun<'_>> = seg.iter().map(|(run, _)| (*run).clone()).collect();
    let range = ModelRange::new(ModelPos::new(node, base), ModelPos::new(node, base));
    let layout = shaper.shape_paragraph(&runs, unwrapped_constraints(), range);
    let Some(line) = layout.lines.into_iter().next() else {
        return Segment::empty();
    };
    let width = line
        .runs
        .iter()
        .map(|r| r.origin.x + advance_of(r))
        .max()
        .unwrap_or(Twip::ZERO);
    let decimal_x = decimal_offset(seg, base).and_then(|target| cluster_x(&line.runs, target));

    Segment {
        runs: line.runs,
        width,
        ascent: line.ascent,
        descent: line.descent,
        decimal_x,
    }
}

/// The node byte offset of the first decimal separator (`.`) in a segment, if any.
fn decimal_offset(seg: &[(&StyledRun<'_>, u32)], _base: u32) -> Option<u32> {
    for (run, offset) in seg {
        if let Some(idx) = run.text.find('.') {
            return Some(offset + idx as u32);
        }
    }
    None
}

/// The left x of the glyph whose cluster anchors at node offset `target`.
fn cluster_x(runs: &[GlyphRun], target: u32) -> Option<Twip> {
    for run in runs {
        let mut x = run.origin.x;
        for glyph in &run.glyphs {
            if glyph.cluster == target {
                return Some(x);
            }
            x = x + glyph.advance;
        }
    }
    None
}

/// The total advance of a glyph run.
fn advance_of(run: &GlyphRun) -> Twip {
    run.glyphs.iter().fold(Twip::ZERO, |acc, g| acc + g.advance)
}

/// A resolved tab stop: its position and how the following text aligns to it.
struct ResolvedStop {
    position: i32,
    alignment: TabAlignment,
    leader: Option<TabLeader>,
}

/// Resolves the tab stop a tab at pen position `pen` advances to: the nearest
/// explicit non-`bar` stop past `pen`, else the next multiple of `default_tab`
/// strictly past `pen`.
fn resolve_next_stop(pen: i32, tab_stops: &[TabStop], default_tab: Twip) -> ResolvedStop {
    let explicit = tab_stops
        .iter()
        .filter(|t| t.alignment != TabAlignment::Bar && t.position_twips > pen)
        .min_by_key(|t| t.position_twips);
    if let Some(stop) = explicit {
        return ResolvedStop {
            position: stop.position_twips,
            alignment: stop.alignment,
            leader: stop.leader,
        };
    }
    // Default tab: the next multiple of the interval strictly greater than `pen`.
    let interval = default_tab.raw().max(1);
    let next = (pen.div_euclid(interval) + 1) * interval;
    ResolvedStop {
        position: next,
        alignment: TabAlignment::Start,
        leader: None,
    }
}

/// Builds a leader glyph run filling the gap `[x0, x1)` on `baseline`, tiling the
/// leader character in the style of the segment it precedes (falling back to the
/// preceding run's style). Returns `None` if the character has no advance or the
/// gap holds no whole glyph.
fn leader_run(
    shaper: &dyn LineShaper,
    block: &Block<'_>,
    seg_index: usize,
    leader: TabLeader,
    x0: Twip,
    x1: Twip,
    baseline: Twip,
) -> Option<GlyphRun> {
    let style = leader_style(block, seg_index)?;
    let ch = leader_char(leader);
    let s = ch.to_string();
    let probe = StyledRun {
        text: &s,
        font: style.font,
        size: style.size,
        bold: false,
        italic: false,
        letter_spacing: Twip::ZERO,
        color: style.color,
        decoration: Decoration::default(),
        highlight: None,
    };
    let dummy = ModelRange::new(
        ModelPos::new(NodeId::from_parts(1, 1).ok()?, 0),
        ModelPos::new(NodeId::from_parts(1, 1).ok()?, 0),
    );
    let layout = shaper.shape_paragraph(&[probe], unwrapped_constraints(), dummy);
    let template = layout.lines.first().and_then(|l| l.runs.first())?;
    let glyph = template.glyphs.first()?;
    let advance = glyph.advance.raw();
    if advance <= 0 {
        return None;
    }
    let gap = x1.raw() - x0.raw();
    let count = (gap / advance) as usize;
    if count == 0 {
        return None;
    }
    let glyphs: Vec<Glyph> = (0..count)
        .map(|_| Glyph {
            id: glyph.id,
            advance: Twip(advance),
            cluster: block.start_offset,
        })
        .collect();
    Some(GlyphRun {
        font: template.font,
        size: template.size,
        color: style.color,
        origin: Point::new(x0, baseline),
        bidi_level: 0,
        decoration: Decoration::default(),
        highlight: None,
        glyphs,
    })
}

/// Picks the style for a tab's leader: the style of the following segment's first
/// run, else the preceding segment's last run.
fn leader_style<'a>(block: &'a Block<'a>, seg_index: usize) -> Option<&'a StyledRun<'a>> {
    if let Some((run, _)) = block.segments.get(seg_index).and_then(|s| s.first()) {
        return Some(run);
    }
    block
        .segments
        .get(seg_index.wrapping_sub(1))
        .and_then(|s| s.last())
        .map(|(run, _)| *run)
}

/// The glyph tiled to draw a leader.
fn leader_char(leader: TabLeader) -> char {
    match leader {
        TabLeader::Dot => '.',
        TabLeader::Hyphen => '-',
        TabLeader::Underscore | TabLeader::Heavy => '_',
        TabLeader::MiddleDot => '\u{00B7}',
    }
}

/// Constraints for measuring/probing a single unwrapped segment: effectively
/// infinite width, no wrap, start-aligned, no indent.
fn unwrapped_constraints() -> LineConstraints {
    LineConstraints {
        max_width: Twip(1_000_000),
        ..LineConstraints::default()
    }
}

/// An empty line anchored at `offset` (a paragraph with no visible content).
fn empty_line(node: NodeId, offset: u32, bars: Vec<Twip>) -> Line {
    Line {
        runs: Vec::new(),
        ascent: Twip::ZERO,
        descent: Twip::ZERO,
        height: Twip::ZERO,
        range: ModelRange::new(ModelPos::new(node, offset), ModelPos::new(node, offset)),
        line_break: LineBreak::ParagraphEnd,
        page_break_after: false,
        bars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use crate::text::FontId;
    use casual_doc_model::NodeId;

    fn node() -> NodeId {
        NodeId::from_parts(1, 1).unwrap()
    }

    fn para_range() -> ModelRange {
        ModelRange::new(ModelPos::new(node(), 0), ModelPos::new(node(), 0))
    }

    fn styled(text: &str) -> StyledRun<'_> {
        StyledRun {
            text,
            font: FontId(0),
            size: Twip::from_points(11),
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
            highlight: None,
        }
    }

    fn stop(position: i32, alignment: TabAlignment, leader: Option<TabLeader>) -> TabStop {
        TabStop {
            position_twips: position,
            alignment,
            leader,
        }
    }

    /// The full advance of a placed glyph run.
    fn run_width(run: &GlyphRun) -> i32 {
        advance_of(run).raw()
    }

    fn constraints() -> LineConstraints {
        LineConstraints {
            max_width: Twip::from_points(600),
            ..LineConstraints::default()
        }
    }

    #[test]
    fn left_tab_jumps_to_the_next_stop() {
        let shaper = ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("A")),
            FlowItem::Tab,
            FlowItem::Run(styled("B")),
        ];
        let stops = vec![stop(2000, TabAlignment::Start, None)];
        let layout = shape_with_flow(
            &shaper,
            &items,
            &stops,
            DEFAULT_TAB_STOP,
            constraints(),
            para_range(),
        );
        assert_eq!(layout.lines.len(), 1);
        // The "B" run (the last) starts at the tab stop's position.
        let b = layout.lines[0].runs.last().unwrap();
        assert!(
            (b.origin.x.raw() - 2000).abs() <= 20,
            "B left edge at the stop (2000), got {}",
            b.origin.x.raw()
        );
    }

    #[test]
    fn right_tab_right_aligns_the_following_text() {
        let shaper = ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("A")),
            FlowItem::Tab,
            FlowItem::Run(styled("Bee")),
        ];
        let stops = vec![stop(2000, TabAlignment::End, None)];
        let layout = shape_with_flow(
            &shaper,
            &items,
            &stops,
            DEFAULT_TAB_STOP,
            constraints(),
            para_range(),
        );
        let b = layout.lines[0].runs.last().unwrap();
        let right_edge = b.origin.x.raw() + run_width(b);
        assert!(
            (right_edge - 2000).abs() <= 20,
            "the text's right edge aligns to the stop (2000), got {right_edge}"
        );
    }

    #[test]
    fn decimal_tab_aligns_the_decimal_point() {
        let shaper = ParleyShaper::new();
        let items = vec![FlowItem::Tab, FlowItem::Run(styled("12.34"))];
        let stops = vec![stop(2000, TabAlignment::Decimal, None)];
        let layout = shape_with_flow(
            &shaper,
            &items,
            &stops,
            DEFAULT_TAB_STOP,
            constraints(),
            para_range(),
        );
        // The '.' is byte offset 2 within "12.34"; its glyph's absolute x should
        // land on the stop.
        let dot_x = cluster_x(&layout.lines[0].runs, 2).expect("decimal glyph found");
        assert!(
            (dot_x.raw() - 2000).abs() <= 25,
            "the decimal point aligns to the stop (2000), got {}",
            dot_x.raw()
        );
    }

    #[test]
    fn dot_leader_fills_the_gap() {
        let shaper = ParleyShaper::new();
        let items = || {
            vec![
                FlowItem::Run(styled("A")),
                FlowItem::Tab,
                FlowItem::Run(styled("B")),
            ]
        };
        let glyphs = |leader: Option<TabLeader>| {
            let stops = vec![stop(4000, TabAlignment::Start, leader)];
            shape_with_flow(
                &shaper,
                &items(),
                &stops,
                DEFAULT_TAB_STOP,
                constraints(),
                para_range(),
            )
            .lines[0]
                .runs
                .iter()
                .map(|r| r.glyphs.len())
                .sum::<usize>()
        };
        assert!(
            glyphs(Some(TabLeader::Dot)) > glyphs(None) + 3,
            "a dot leader adds many glyphs across the gap"
        );
    }

    #[test]
    fn default_tab_stop_is_used_when_no_explicit_stop_covers_the_position() {
        let shaper = ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("A")),
            FlowItem::Tab,
            FlowItem::Run(styled("B")),
        ];
        // No explicit stops: the tab advances to the next default multiple (1440).
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[],
            Twip(1440),
            constraints(),
            para_range(),
        );
        let b = layout.lines[0].runs.last().unwrap();
        assert!(
            (b.origin.x.raw() - 1440).abs() <= 20,
            "B advances to the default stop (1440), got {}",
            b.origin.x.raw()
        );
    }

    #[test]
    fn a_textwrapping_break_starts_a_new_line() {
        let shaper = ParleyShaper::new();
        // `w:cr` and `w:br` (textWrapping) both map to `BreakKind::Line`.
        let items = vec![
            FlowItem::Run(styled("A")),
            FlowItem::Break(BreakKind::Line),
            FlowItem::Run(styled("B")),
        ];
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[],
            DEFAULT_TAB_STOP,
            constraints(),
            para_range(),
        );
        assert_eq!(layout.lines.len(), 2, "one run pair split into two lines");
        assert_eq!(layout.lines[0].line_break, LineBreak::Hard);
        assert!(
            !layout.lines[0].page_break_after,
            "a line break is not a page break"
        );
        assert_eq!(layout.lines[1].line_break, LineBreak::ParagraphEnd);
        // The second line sits below the first.
        let y0 = layout.lines[0].runs[0].origin.y.raw();
        let y1 = layout.lines[1].runs[0].origin.y.raw();
        assert!(y1 > y0, "the second line is stacked below the first");
    }

    #[test]
    fn a_page_break_marks_the_line_for_the_paginator() {
        let shaper = ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("A")),
            FlowItem::Break(BreakKind::Page),
            FlowItem::Run(styled("B")),
        ];
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[],
            DEFAULT_TAB_STOP,
            constraints(),
            para_range(),
        );
        assert_eq!(layout.lines.len(), 2);
        assert!(
            layout.lines[0].page_break_after,
            "the first line carries the forced page break"
        );
        assert_eq!(layout.lines[0].line_break, LineBreak::Hard);
    }

    #[test]
    fn a_bar_stop_draws_a_rule_on_every_line() {
        let shaper = ParleyShaper::new();
        let items = vec![FlowItem::Run(styled("A"))];
        let stops = vec![stop(1500, TabAlignment::Bar, None)];
        // A bar stop needs the flow layer even without a tab character.
        assert!(needs_flow_layout(&items, &stops));
        let layout = shape_with_flow(
            &shaper,
            &items,
            &stops,
            DEFAULT_TAB_STOP,
            constraints(),
            para_range(),
        );
        assert_eq!(layout.lines[0].bars, vec![Twip(1500)]);
    }

    #[test]
    fn plain_text_needs_no_flow_layer() {
        let items = vec![FlowItem::Run(styled("plain text"))];
        assert!(!needs_flow_layout(&items, &[]));
    }
}
