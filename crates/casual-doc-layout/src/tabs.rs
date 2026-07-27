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
use casual_doc_model::v1::{
    BreakKind, PositionalTabAlignment, PositionalTabLeader, PositionalTabRelativeTo, TabAlignment,
    TabLeader, TabStop,
};

use crate::block::BlockFragment;
use crate::model::{ModelPos, ModelRange};
use crate::text::{
    Decoration, FieldKind, FieldStyle, Glyph, GlyphRun, InlineFloatSide, InlineRule, Line,
    LineBreak, LineConstraints, LineLayout, LineShaper, StyledRun, TextAlignment,
    TextBoxContentLayout, TextBoxStroke,
};
use crate::units::{Point, Size, Twip};

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
    /// An absolute positional tab (`w:ptab`). Unlike [`FlowItem::Tab`], the stop
    /// is selected directly from the paragraph indent or text-margin box.
    PositionalTab {
        /// How the following segment aligns at the selected edge/center.
        alignment: PositionalTabAlignment,
        /// Whether the target box is the paragraph indent or text margins.
        relative_to: PositionalTabRelativeTo,
        /// Optional leader drawn across the tab advance.
        leader: PositionalTabLeader,
    },
    /// A hard break (`w:br`/`w:cr`) with its kind.
    Break(BreakKind),
    /// An inline embedded picture (`w:drawing`/`wp:inline`): its resolved media
    /// key (part name) and box size (twips, from the EMU extent). Laid out as an
    /// inline box on its own line; the renderer resolves the media to pixels.
    Image {
        /// The media part name — the display list's stable media key.
        media: String,
        /// The image box size in twips (from the drawing's EMU extent).
        size: Size,
    },
    /// An inline field (`w:fldSimple`/`w:instrText`): its resolved kind, a
    /// placeholder value (the producer's cached result) shaped at flow time, and
    /// the styling to reshape a recomputed value. Laid out inline like a run; the
    /// post-pagination field pass stamps `PAGE`/`NUMPAGES` values.
    Field {
        /// What value the field yields.
        kind: FieldKind,
        /// The placeholder display value (the cached result text) shaped now.
        value: String,
        /// The run styling, so the field pass can reshape a new value.
        style: FieldStyle,
    },
    /// An inline text box (`wps:txbx` / `v:textbox`): its recursive block content
    /// already flowed through the shared pipeline into fragments, the box's outer
    /// size (twips), and its border/fill (RGBA). Laid out as an inline box on its
    /// own line, like an [`FlowItem::Image`]; composition paints the box and its
    /// flowed content.
    TextBox {
        /// The flowed block fragments (the box's content).
        blocks: Vec<BlockFragment>,
        /// The box's resolved outer size in twips.
        size: Size,
        /// The authored border color and width, or `None` for no border.
        border: Option<TextBoxStroke>,
        /// The background fill (RGBA), or `None` for transparent.
        fill: Option<[u8; 4]>,
        /// Resolved content offset and overflow clipping.
        content_layout: TextBoxContentLayout,
    },
    /// An inline horizontal rule (`w:pict` / `v:rect@o:hr`): a filled full-content-
    /// width line, already resolved (origin, size, color) against the content width
    /// and alignment. Laid out on its own line, like an [`FlowItem::Image`];
    /// composition paints it as a filled rect.
    HorizontalRule(InlineRule),
    /// A non-painting vertical exclusion introduced by a floating object with
    /// `wrapTopAndBottom`. It reserves `height` at the paragraph start so the
    /// paragraph's visible content begins below the float.
    FloatBarrier {
        /// Required clearance in twips.
        height: Twip,
    },
    /// A paragraph-local square/tight/through wrap exclusion. The anchored object
    /// is painted separately; this marker narrows intersecting text lines.
    FloatExclusion {
        /// Inline edge occupied by the anchored object.
        side: InlineFloatSide,
        /// Horizontal exclusion including wrap distances.
        width: Twip,
        /// Vertical clearance from the anchor paragraph's top.
        height: Twip,
    },
}

/// Whether an item stream needs the tab/break layer at all: any tab, any hard
/// break, or any `bar` tab stop (which draws even without a tab character). When
/// this is `false`, the caller uses the base shaper directly.
#[must_use]
pub fn needs_flow_layout(items: &[FlowItem<'_>], tab_stops: &[TabStop]) -> bool {
    items.iter().any(|i| {
        matches!(
            i,
            FlowItem::Tab | FlowItem::PositionalTab { .. } | FlowItem::Break(_)
        )
    }) || tab_stops.iter().any(|t| t.alignment == TabAlignment::Bar)
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

/// One tabbed line under construction. Vertical positions are normalized after
/// every segment has been placed, because a later tab segment can raise the
/// line's ascent/descent.
struct TabLine {
    runs: Vec<GlyphRun>,
    ascent: Twip,
    descent: Twip,
    range: ModelRange,
}

#[derive(Clone, Copy)]
struct TabWrap {
    line_limit: Twip,
    left: Twip,
    keep_hanging_column: bool,
}

impl TabLine {
    fn empty(node: NodeId, offset: u32) -> Self {
        Self {
            runs: Vec::new(),
            ascent: Twip::ZERO,
            descent: Twip::ZERO,
            range: ModelRange::new(ModelPos::new(node, offset), ModelPos::new(node, offset)),
        }
    }

    fn right_edge(&self) -> Twip {
        self.runs
            .iter()
            .map(|run| run.origin.x + advance_of(run))
            .max()
            .unwrap_or(Twip::ZERO)
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

        let mut lines = if !block.tabs.is_empty() {
            layout_tabbed_line(
                shaper,
                node,
                block,
                tab_stops,
                default_tab,
                first_line_indent,
                constraints,
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
                    last.line_break = match kind {
                        BreakKind::Line => LineBreak::Hard,
                        BreakKind::Page => LineBreak::Page,
                        BreakKind::Column => LineBreak::Column,
                    };
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
    /// The tab boundary preceding each segment after the first.
    /// `tabs.len() + 1 == segments.len()`.
    tabs: Vec<TabKind>,
    /// Node byte offset at which the block starts (for line ranges).
    start_offset: u32,
    /// Node byte offset at which the block ends.
    end_offset: u32,
    /// The break that ends this block.
    trailing: Option<BreakKind>,
}

/// A tab boundary retained while splitting a hard-break block.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TabKind {
    /// Ordinary `w:tab`, resolved through explicit/default tab stops.
    Ordinary,
    /// Absolute `w:ptab`, resolved directly against a box edge or center.
    Positional {
        alignment: PositionalTabAlignment,
        relative_to: PositionalTabRelativeTo,
        leader: PositionalTabLeader,
    },
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
    let mut tabs = Vec::new();

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
                tabs.push(TabKind::Ordinary);
                segments.push(Vec::new());
            }
            FlowItem::PositionalTab {
                alignment,
                relative_to,
                leader,
            } => {
                tabs.push(TabKind::Positional {
                    alignment: *alignment,
                    relative_to: *relative_to,
                    leader: *leader,
                });
                segments.push(Vec::new());
            }
            FlowItem::Break(kind) => {
                blocks.push(Block {
                    segments: std::mem::replace(&mut segments, vec![Vec::new()]),
                    tabs: std::mem::take(&mut tabs),
                    start_offset,
                    end_offset: byte,
                    trailing: Some(*kind),
                });
                start_offset = byte;
            }
            // Inline images, fields, text boxes, and float barriers are handled by their own
            // layout paths before the stream reaches the tab/break layer, so none
            // reach here.
            FlowItem::Image { .. }
            | FlowItem::Field { .. }
            | FlowItem::TextBox { .. }
            | FlowItem::HorizontalRule(_)
            | FlowItem::FloatBarrier { .. }
            | FlowItem::FloatExclusion { .. } => {}
        }
    }
    blocks.push(Block {
        segments,
        tabs,
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

/// Lays out a block that contains tabs, resolving each tab to a horizontal
/// advance and positioning the following segment by the stop's alignment,
/// filling leaders in the gap.
///
/// Common TOC/form rows remain one line when they fit. If the trailing value
/// exceeds the available measure, it soft-wraps at its resolved tab column and
/// continuation lines keep that hanging column. This prevents long tabbed form
/// values from escaping the page without disturbing right-aligned TOC numbers.
fn layout_tabbed_line(
    shaper: &dyn LineShaper,
    node: NodeId,
    block: &Block<'_>,
    tab_stops: &[TabStop],
    default_tab: Twip,
    first_line_indent: Twip,
    constraints: LineConstraints,
) -> Vec<Line> {
    // Measure every segment unwrapped.
    let segments: Vec<Segment> = block
        .segments
        .iter()
        .map(|seg| measure_segment(shaper, node, seg))
        .collect();
    let mut lines = vec![TabLine::empty(node, block.start_offset)];
    // A hanging indent deliberately starts the first line left of the
    // paragraph's normal text origin. Keeping that negative local coordinate is
    // essential for form rows whose tab stops are authored relative to the text
    // margin: clamping it to zero shifts the label and every tabbed value right.
    let mut pen = first_line_indent.raw();

    for (i, seg) in segments.iter().enumerate() {
        let mut left = pen;
        let mut leader = None;
        if i > 0 {
            // A tab precedes segment `i`: resolve the stop it advances to.
            let stop = resolve_stop(block.tabs[i - 1], pen, tab_stops, default_tab, constraints);
            // Where the following segment's box is placed relative to the stop.
            left = match stop.alignment {
                TabAlignment::Start | TabAlignment::Bar => stop.position,
                TabAlignment::End => stop.position - seg.width.raw(),
                TabAlignment::Center => stop.position - seg.width.raw() / 2,
                TabAlignment::Decimal => stop.position - seg.decimal_x.unwrap_or(seg.width).raw(),
            };
            // Never overlap the preceding content.
            if left < pen {
                left = pen;
            }
            leader = stop.leader;
        }

        if let Some(leader) = leader
            && left > pen
            && let Some(run) =
                leader_run(shaper, block, i, leader, Twip(pen), Twip(left), Twip::ZERO)
        {
            lines
                .last_mut()
                .expect("tabbed line list is never empty")
                .runs
                .push(run);
        }

        // A margin-relative positional tab is explicitly allowed to target the
        // text-margin box beyond a paragraph's trailing indent. Ordinary tabs and
        // indent-relative positional tabs remain bounded by the paragraph measure.
        let line_limit = if i > 0
            && matches!(
                block.tabs[i - 1],
                TabKind::Positional {
                    relative_to: PositionalTabRelativeTo::Margin,
                    ..
                }
            ) {
            (constraints.margin_width - constraints.indent_start).max(constraints.max_width)
        } else {
            constraints.max_width
        };
        let available = line_limit.raw() - left;
        let overflowing =
            !block.segments[i].is_empty() && (available <= 0 || seg.width.raw() > available);
        if overflowing {
            let is_final = i + 1 == segments.len();
            let keep_hanging_column = is_final && available > 0;
            let wrap_left = if available > 0 {
                Twip(left)
            } else {
                Twip::ZERO
            };
            let merge_first = available > 0;
            let shaped = shape_wrapped_tab_segment(
                shaper,
                node,
                block,
                i,
                constraints,
                TabWrap {
                    line_limit,
                    left: wrap_left,
                    keep_hanging_column,
                },
            );
            append_wrapped_tab_segment(&mut lines, shaped, merge_first);
            pen = lines.last().map_or(0, |line| line.right_edge().raw());
            continue;
        }

        let current = lines.last_mut().expect("tabbed line list is never empty");
        push_shifted(&mut current.runs, &seg.runs, Twip(left), Twip::ZERO);
        current.ascent = current.ascent.max(seg.ascent);
        current.descent = current.descent.max(seg.descent);
        current.range.end = ModelPos::new(node, segment_end(block, i));
        pen = left + seg.width.raw();
    }

    let line_count = lines.len();
    let mut cursor_y = Twip::ZERO;
    lines
        .into_iter()
        .enumerate()
        .map(|(index, mut work)| {
            // Apply the paragraph's authored line rule once, after every segment
            // contributing to this physical line has established its natural box.
            let (ascent, descent, height) =
                assembled_line_metrics(work.ascent, work.descent, constraints);
            for run in &mut work.runs {
                run.origin.y = cursor_y + ascent;
            }
            let line = Line {
                runs: work.runs,
                ascent,
                descent,
                height,
                clip: constraints.line_exact.is_some(),
                range: work.range,
                line_break: if index + 1 == line_count {
                    LineBreak::ParagraphEnd
                } else {
                    LineBreak::Wrap
                },
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                text_boxes: Vec::new(),
                rules: Vec::new(),
            };
            cursor_y = cursor_y + height;
            line
        })
        .collect()
}

/// Shapes one overflowing tab segment. A final segment retains its hanging tab
/// column on continuation lines. An intermediate segment uses the tab position as
/// a first-line indent, then resumes at the paragraph start so later tab stops can
/// form the next logical row (the SDS label/value pattern).
fn shape_wrapped_tab_segment(
    shaper: &dyn LineShaper,
    node: NodeId,
    block: &Block<'_>,
    segment_index: usize,
    constraints: LineConstraints,
    wrap: TabWrap,
) -> Vec<Line> {
    let segment = &block.segments[segment_index];
    let start = segment
        .first()
        .map_or(block.end_offset, |(_, offset)| *offset);
    let runs: Vec<StyledRun<'_>> = segment.iter().map(|(run, _)| (*run).clone()).collect();
    let available = Twip((wrap.line_limit.raw() - wrap.left.raw()).max(1));
    let wrap_constraints = LineConstraints {
        max_width: if wrap.keep_hanging_column {
            available
        } else {
            wrap.line_limit
        },
        margin_width: if wrap.keep_hanging_column {
            available
        } else {
            constraints.margin_width
        },
        indent_start: Twip::ZERO,
        alignment: TextAlignment::Start,
        line_height_percent: None,
        line_at_least: None,
        line_exact: None,
        first_line_indent: if wrap.keep_hanging_column {
            Twip::ZERO
        } else {
            wrap.left
        },
        ..constraints
    };
    let mut lines = shaper
        .shape_paragraph(
            &runs,
            wrap_constraints,
            ModelRange::new(
                ModelPos::new(node, start),
                ModelPos::new(node, segment_end(block, segment_index)),
            ),
        )
        .lines;
    if wrap.keep_hanging_column {
        for line in &mut lines {
            for run in &mut line.runs {
                run.origin.x = run.origin.x + wrap.left;
            }
        }
    }
    lines
}

/// Merges the first shaped line with the already positioned tab prefix, then
/// appends any continuation lines as new physical lines.
fn append_wrapped_tab_segment(out: &mut Vec<TabLine>, mut shaped: Vec<Line>, merge_first: bool) {
    if shaped.is_empty() {
        return;
    }
    if merge_first {
        let first = shaped.remove(0);
        let current = out.last_mut().expect("tabbed line list is never empty");
        current.ascent = current.ascent.max(first.ascent);
        current.descent = current.descent.max(first.descent);
        current.range.end = first.range.end;
        current.runs.extend(first.runs);
    }
    for mut line in shaped {
        for run in &mut line.runs {
            run.origin.y = Twip::ZERO;
        }
        out.push(TabLine {
            runs: line.runs,
            ascent: line.ascent,
            descent: line.descent,
            range: line.range,
        });
    }
}

fn segment_end(block: &Block<'_>, segment_index: usize) -> u32 {
    block.segments[segment_index]
        .last()
        .map_or(block.start_offset, |(run, offset)| {
            offset.saturating_add(run.text.len() as u32)
        })
}

/// Resolves the authored line rule for a line assembled outside the base shaper.
fn assembled_line_metrics(
    ascent: Twip,
    descent: Twip,
    constraints: LineConstraints,
) -> (Twip, Twip, Twip) {
    let natural = Twip(ascent.raw() + descent.raw());
    // `lineRule="auto"` with an explicit `w:line`: scale the single-line box by
    // the percent, with positive leading below the baseline.
    let (ascent, descent, natural) = match constraints.line_height_percent {
        Some(percent) if percent != 100 => {
            let scaled = (natural.raw() as i64 * percent as i64 / 100) as i32;
            let extra = (scaled - natural.raw()).max(0);
            (
                ascent,
                Twip(descent.raw() + extra),
                Twip(natural.raw() + extra),
            )
        }
        _ => (ascent, descent, natural),
    };
    crate::shape::apply_line_rule(ascent, descent, natural, &constraints)
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
pub(crate) struct ResolvedStop {
    pub(crate) position: i32,
    pub(crate) alignment: TabAlignment,
    pub(crate) leader: Option<TabLeader>,
}

/// Resolves either an ordinary or absolute positional tab into the local
/// coordinate system used by the paragraph shaper.
pub(crate) fn resolve_stop(
    tab: TabKind,
    pen: i32,
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
) -> ResolvedStop {
    let TabKind::Positional {
        alignment,
        relative_to,
        leader,
    } = tab
    else {
        // Ordinary `w:tabs/@w:pos` coordinates are relative to the text margin,
        // while this shaper's origin is already moved to the paragraph's start
        // indent. Resolve in margin coordinates, then translate the stop back
        // into the indent-local line coordinates. This is observable in hanging
        // form rows where `w:left=3612`, `w:hanging=3049`, and the value stop is
        // exactly `w:pos=3612`.
        let absolute_pen = pen + constraints.indent_start.raw();
        let mut stop = resolve_next_stop(absolute_pen, tab_stops, default_tab);
        stop.position -= constraints.indent_start.raw();
        return stop;
    };

    // `margin_width == 0` keeps hand-built/default constraints backwards
    // compatible: in that case the indented wrap box is the only known box.
    let margin_width = if constraints.margin_width > Twip::ZERO {
        constraints.margin_width
    } else {
        constraints.max_width
    };
    let (left, right) = match relative_to {
        PositionalTabRelativeTo::Indent => (0, constraints.max_width.raw()),
        PositionalTabRelativeTo::Margin => (
            -constraints.indent_start.raw(),
            margin_width.raw() - constraints.indent_start.raw(),
        ),
    };
    let position = match alignment {
        PositionalTabAlignment::Left => left,
        PositionalTabAlignment::Center => left + (right - left) / 2,
        PositionalTabAlignment::Right => right,
    };
    let alignment = match alignment {
        PositionalTabAlignment::Left => TabAlignment::Start,
        PositionalTabAlignment::Center => TabAlignment::Center,
        PositionalTabAlignment::Right => TabAlignment::End,
    };
    let leader = match leader {
        PositionalTabLeader::None => None,
        PositionalTabLeader::Dot => Some(TabLeader::Dot),
        PositionalTabLeader::Hyphen => Some(TabLeader::Hyphen),
        PositionalTabLeader::Underscore => Some(TabLeader::Underscore),
        PositionalTabLeader::MiddleDot => Some(TabLeader::MiddleDot),
    };
    ResolvedStop {
        position,
        alignment,
        leader,
    }
}

/// Resolves the tab stop a tab at pen position `pen` advances to: the nearest
/// explicit non-`bar` stop past `pen`, else the next multiple of `default_tab`
/// strictly past `pen`.
pub(crate) fn resolve_next_stop(
    pen: i32,
    tab_stops: &[TabStop],
    default_tab: Twip,
) -> ResolvedStop {
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
        text: s.as_str().into(),
        // A tab leader's glyph uses the resolved face; no declared family to prefer.
        requested_family: None,
        font: style.font,
        size: style.size,
        character_scale_percent: style.character_scale_percent,
        bold: false,
        italic: false,
        letter_spacing: Twip::ZERO,
        color: style.color,
        decoration: Decoration::default(),
        highlight: None,
        baseline_shift: Twip::ZERO,
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
        character_scale_percent: template.character_scale_percent,
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
pub(crate) fn unwrapped_constraints() -> LineConstraints {
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
        clip: false,
        range: ModelRange::new(ModelPos::new(node, offset), ModelPos::new(node, offset)),
        line_break: LineBreak::ParagraphEnd,
        page_break_after: false,
        bars,
        images: Vec::new(),
        fields: Vec::new(),
        text_boxes: Vec::new(),
        rules: Vec::new(),
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
            text: text.into(),
            requested_family: None,
            font: FontId(0),
            size: Twip::from_points(11),
            character_scale_percent: 100,
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
            highlight: None,
            baseline_shift: Twip::ZERO,
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
    fn margin_relative_positional_tab_right_aligns_at_the_text_margin() {
        let shaper = ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("Heading")),
            FlowItem::PositionalTab {
                alignment: PositionalTabAlignment::Right,
                relative_to: PositionalTabRelativeTo::Margin,
                leader: PositionalTabLeader::Dot,
            },
            FlowItem::Run(styled("23")),
        ];
        let constraints = LineConstraints {
            // The paragraph is indented 1000 twips from each side of an
            // 8000-twip text-margin box.
            max_width: Twip(6000),
            margin_width: Twip(8000),
            indent_start: Twip(1000),
            ..LineConstraints::default()
        };
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[],
            DEFAULT_TAB_STOP,
            constraints,
            para_range(),
        );
        let page_number = layout.lines[0].runs.last().unwrap();
        let right_edge = page_number.origin.x.raw() + run_width(page_number);
        assert!(
            (right_edge - 7000).abs() <= 20,
            "the local right edge is the margin edge minus the start indent, got {right_edge}"
        );
        assert!(
            layout.lines[0].runs.len() > 2,
            "the positional tab emits a dot-leader run"
        );
    }

    #[test]
    fn an_exact_tabbed_line_reanchors_its_baseline_inside_the_clip_box() {
        let shaper = crate::shape::ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("left")),
            FlowItem::Tab,
            FlowItem::Run(styled("right")),
        ];
        let exact = Twip(120);
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[TabStop {
                position_twips: 1800,
                alignment: TabAlignment::Start,
                leader: None,
            }],
            DEFAULT_TAB_STOP,
            LineConstraints {
                max_width: Twip(4000),
                line_exact: Some(exact),
                ..LineConstraints::default()
            },
            para_range(),
        );
        let line = &layout.lines[0];
        assert_eq!(line.height, exact);
        assert!(line.clip);
        assert!(
            line.runs
                .iter()
                .all(|run| (0..=exact.raw()).contains(&run.origin.y.raw()))
        );
    }

    #[test]
    fn an_overflowing_tabbed_value_wraps_at_its_hanging_column() {
        let shaper = crate::shape::ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("label")),
            FlowItem::Tab,
            FlowItem::Run(styled("装有纯水的洗眼瓶 紧密贴合的防护眼罩")),
        ];
        let max_width = Twip(2200);
        let tab_column = 700;
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[TabStop {
                position_twips: tab_column,
                alignment: TabAlignment::Start,
                leader: None,
            }],
            DEFAULT_TAB_STOP,
            LineConstraints {
                max_width,
                ..LineConstraints::default()
            },
            para_range(),
        );

        assert!(
            layout.lines.len() >= 2,
            "the long trailing value must soft-wrap"
        );
        for line in &layout.lines {
            for run in &line.runs {
                assert!(
                    run.origin.x.raw() + run_width(run) <= max_width.raw() + 2,
                    "wrapped tab content escaped the available measure"
                );
            }
        }
        assert!(
            layout.lines[1]
                .runs
                .iter()
                .all(|run| run.origin.x.raw() >= tab_column),
            "continuation text must retain the resolved tab column"
        );
    }

    #[test]
    fn an_overflowing_intermediate_tab_segment_resets_before_later_tab_content() {
        let shaper = crate::shape::ParleyShaper::new();
        let middle = "这是需要换行的较长中间字段内容";
        let items = vec![
            FlowItem::Run(styled("label")),
            FlowItem::Tab,
            FlowItem::Run(styled(middle)),
            FlowItem::Tab,
            FlowItem::Run(styled("tail")),
        ];
        let max_width = Twip(2_400);
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[
                stop(700, TabAlignment::Start, None),
                stop(1_800, TabAlignment::Start, None),
            ],
            DEFAULT_TAB_STOP,
            LineConstraints {
                max_width,
                ..LineConstraints::default()
            },
            para_range(),
        );

        assert!(
            layout.lines.len() >= 2,
            "the intermediate field must soft-wrap"
        );
        assert!(
            layout.lines[1]
                .runs
                .iter()
                .any(|run| run.origin.x.raw().abs() <= 2),
            "an intermediate field resumes at the paragraph start, not at its tab column"
        );
        for line in &layout.lines {
            for run in &line.runs {
                assert!(
                    run.origin.x.raw() + run_width(run) <= max_width.raw() + 2,
                    "a multi-tab continuation escaped the available measure"
                );
            }
        }

        let tail_offset = ("label".len() + middle.len()) as u32;
        let tail = layout
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .find(|run| {
                run.glyphs
                    .first()
                    .is_some_and(|glyph| glyph.cluster >= tail_offset)
            })
            .expect("tail run after the later tab");
        let tail_x = tail.origin.x.raw();
        assert!(
            tail_x.abs() <= 2 || (tail_x - 1_800).abs() <= 20,
            "later tab content must use the remaining stop or a contained logical row; got {tail_x}"
        );
    }

    #[test]
    fn ordinary_tabs_translate_from_margin_to_hanging_indent_coordinates() {
        let shaper = crate::shape::ParleyShaper::new();
        let items = vec![
            FlowItem::Run(styled("eye protection")),
            FlowItem::Tab,
            FlowItem::Run(styled(":")),
            FlowItem::Tab,
            FlowItem::Run(styled(
                "pure-water eyewash bottle close-fitting protective goggles",
            )),
        ];
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[
                TabStop {
                    position_twips: 3328,
                    alignment: TabAlignment::Start,
                    leader: None,
                },
                TabStop {
                    position_twips: 3612,
                    alignment: TabAlignment::Start,
                    leader: None,
                },
            ],
            DEFAULT_TAB_STOP,
            LineConstraints {
                max_width: Twip(1803),
                margin_width: Twip(9650),
                indent_start: Twip(3612),
                first_line_indent: Twip(-3049),
                ..LineConstraints::default()
            },
            para_range(),
        );

        assert!(layout.lines.len() >= 2);
        assert!(
            layout.lines[0].runs[0].origin.x < Twip::ZERO,
            "the hanging label must protrude left of the normal indent"
        );
        let value_start = layout.lines[0]
            .runs
            .iter()
            .find(|run| run.glyphs.first().is_some_and(|glyph| glyph.cluster >= 15))
            .expect("value run");
        assert!(
            value_start.origin.x.raw().abs() <= 2,
            "the second stop equals the paragraph indent and should be local x=0"
        );
        assert!(
            layout.lines[1]
                .runs
                .iter()
                .all(|run| run.origin.x >= Twip::ZERO)
        );
    }

    #[test]
    fn indent_relative_positional_tab_centers_on_the_indented_column() {
        let shaper = ParleyShaper::new();
        let items = vec![
            FlowItem::PositionalTab {
                alignment: PositionalTabAlignment::Center,
                relative_to: PositionalTabRelativeTo::Indent,
                leader: PositionalTabLeader::None,
            },
            FlowItem::Run(styled("center")),
        ];
        let constraints = LineConstraints {
            max_width: Twip(6000),
            margin_width: Twip(8000),
            indent_start: Twip(1000),
            ..LineConstraints::default()
        };
        let layout = shape_with_flow(
            &shaper,
            &items,
            &[],
            DEFAULT_TAB_STOP,
            constraints,
            para_range(),
        );
        let run = layout.lines[0].runs.last().unwrap();
        let center = run.origin.x.raw() + run_width(run) / 2;
        assert!(
            (center - 3000).abs() <= 20,
            "the segment is centered in the 6000-twip indented column, got {center}"
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
        assert_eq!(layout.lines[0].line_break, LineBreak::Page);
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
