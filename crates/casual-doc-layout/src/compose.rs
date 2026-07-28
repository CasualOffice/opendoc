//! Composition — turning a shaped [`LineLayout`] into a [`DisplayList`].
//!
//! This is the seam between layout and rendering: each shaped glyph run is
//! translated to its position on the page and emitted as a paint item. The list
//! stays in device-independent twips (consistent with the whole engine); the
//! rendering backend applies the device scale (DPI × zoom) when it paints, which
//! is the "scale only at paint" rule from `43-…`.

use crate::block::{
    BlockFragment, BorderPattern, CellBorders, CellVerticalMerge, ParagraphDecor,
    ResolvedBorderSegment, ResolvedEdge,
};
use crate::display::{Color, DisplayList, PaintItem, Stroke};
use crate::page::{AnchorContent, Page, PlacedAnchor};
use crate::text::{LineLayout, TextBoxContentLayout};
use crate::units::{Point, Rect, Size, Twip};

/// Width (twips) of a `bar` tab stop's vertical rule (~0.5pt, Word's hairline).
const BAR_TAB_WIDTH: Twip = Twip(10);

/// Width (twips) of a column separator rule (`w:cols/@w:sep`) — Word's ~0.5pt
/// hairline, the same weight as a bar-tab rule.
const COLUMN_SEPARATOR_WIDTH: Twip = Twip(10);

/// Stroke width (device px) of an inline text box's border (a hairline).
/// Builds a display list for one paragraph's shaped lines, placed with the
/// paragraph's top-left at `origin` (in twips). The shaper positions each glyph
/// run relative to the paragraph's own origin (run `origin` = the run's left edge
/// on its baseline); composition translates those into page coordinates.
#[must_use]
pub fn compose_paragraph(layout: &LineLayout, origin: Point) -> DisplayList {
    let mut list = DisplayList::new();
    // Tracks the top of the current line (twips from the paragraph content top) so
    // `bar` tab stops can be drawn as vertical rules spanning the line's box.
    let mut line_top = Twip::ZERO;
    for line in &layout.lines {
        let current_top = line_top;
        if line.clip {
            // Exact line spacing clips only the block axis. Use a deliberately
            // huge horizontal span so hanging indents and positioned tabs remain
            // visible while glyph ink cannot escape into the next line.
            const HORIZONTAL_CLIP_EXTENT: Twip = Twip(1 << 27);
            list.push(PaintItem::PushClip(Rect::new(
                Point::new(origin.x - HORIZONTAL_CLIP_EXTENT, origin.y + current_top),
                Size::new(HORIZONTAL_CLIP_EXTENT + HORIZONTAL_CLIP_EXTENT, line.height),
            )));
        }
        // Bar tab stops (`w:tab@val="bar"`): a thin vertical rule at each stop's x,
        // spanning the full line height, painted behind the glyphs.
        for &bar_x in &line.bars {
            list.push(PaintItem::Rect {
                rect: Rect::new(
                    Point::new(origin.x + bar_x, origin.y + line_top),
                    Size::new(BAR_TAB_WIDTH, line.height),
                ),
                fill: Some(Color::BLACK),
                stroke: None,
            });
        }
        line_top = line_top + line.height;
        for run in &line.runs {
            let placed_x = origin.x + run.origin.x;
            let baseline_y = origin.y + run.origin.y;
            // A run highlight (`w:highlight`) fills the run's glyph box *before*
            // the glyphs (behind the text). The box spans the run's total advance
            // horizontally and the line's ascent+descent vertically.
            if let Some(highlight) = run.highlight {
                let advance = run.glyphs.iter().fold(Twip::ZERO, |acc, g| acc + g.advance);
                list.push(PaintItem::Rect {
                    rect: Rect::new(
                        Point::new(placed_x, baseline_y - line.ascent),
                        Size::new(advance, line.ascent + line.descent),
                    ),
                    fill: Some(rgba(highlight)),
                    stroke: None,
                });
            }
            let mut placed = run.clone();
            placed.origin = Point::new(placed_x, baseline_y);
            list.push(PaintItem::Glyphs { run: placed });
        }
        // Inline images (embedded pictures): the box's `origin` is already
        // paragraph-absolute; translate into page space and emit a blit. The
        // backend resolves `media` to pixels and scales them into the box.
        for image in &line.images {
            list.push(PaintItem::Image {
                media: image.media.clone(),
                rect: Rect::new(
                    Point::new(origin.x + image.origin.x, origin.y + image.origin.y),
                    image.size,
                ),
            });
        }
        // Inline text boxes: the fill and border paint first, then the box's flowed
        // fragments compose offset into it by the internal margin — the *same*
        // fragment composition the body and table cells use (the uniform-flow
        // invariant), so a text box renders paragraphs, nested tables, and images.
        for text_box in &line.text_boxes {
            let box_origin = Point::new(origin.x + text_box.origin.x, origin.y + text_box.origin.y);
            let box_rect = Rect::new(box_origin, text_box.size);
            if let Some(fill) = text_box.fill {
                list.push(PaintItem::Rect {
                    rect: box_rect,
                    fill: Some(rgba(fill)),
                    stroke: None,
                });
            }
            if let Some(border) = text_box.border {
                list.push(PaintItem::Rect {
                    rect: box_rect,
                    fill: None,
                    stroke: Some(Stroke {
                        color: rgba(border.color),
                        width: stroke_px(border.width),
                    }),
                });
            }
            let content_origin = Point::new(
                box_origin.x + text_box.content_layout.origin.x,
                box_origin.y + text_box.content_layout.origin.y,
            );
            let clip = text_box_clip(box_rect, text_box.content_layout);
            if let Some(clip) = clip {
                list.push(PaintItem::PushClip(clip));
            }
            compose_blocks(&mut list, &text_box.blocks, content_origin);
            if clip.is_some() {
                list.push(PaintItem::PopClip);
            }
        }
        // Inline horizontal rules (`w:pict` / `v:rect@o:hr`): a filled rectangle
        // spanning (a fraction of) the content width, translated into page space.
        for rule in &line.rules {
            list.push(PaintItem::Rect {
                rect: Rect::new(
                    Point::new(origin.x + rule.origin.x, origin.y + rule.origin.y),
                    rule.size,
                ),
                fill: Some(rgba(rule.color)),
                stroke: None,
            });
        }
        if line.clip {
            list.push(PaintItem::PopClip);
        }
    }
    list
}

/// A shape/line stroke's device-pixel width: the twip width at 96 DPI, floored at
/// a 1px hairline (matching the inline text-box border). A `0`-twip outline (the
/// common thin DrawingML `a:ln`) is a hairline.
fn stroke_px(width: Twip) -> f32 {
    (width.raw() as f32 * 96.0 / 1440.0).max(1.0)
}

/// Builds a [`Color`] from a packed RGBA quad.
fn rgba(c: [u8; 4]) -> Color {
    Color {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}

/// Builds the display list for a whole paginated [`Page`]: the running header, the
/// body content (each placed paragraph or table row), then the running footer —
/// each fragment composed at its position on the page. Header and footer are laid
/// out in their reserved bands by [`crate::running::place_running_content`] and
/// their fields resolved by [`crate::paginate::resolve_fields`]; here they paint
/// exactly like body fragments.
#[must_use]
pub fn compose_page(page: &Page) -> DisplayList {
    let mut list = DisplayList::new();
    // The float layer is a single stable z-order: `behindDoc` floats paint below
    // the text layer, the rest above, each band ordered by (relativeHeight,
    // document order) so group children paint in child order and a shape can sit
    // behind the group's picture while a later shape sits in front.
    let mut floats: Vec<&PlacedAnchor> = page.anchored.iter().collect();
    floats.sort_by_key(|anchor| anchor.z);
    for anchor in floats.iter().filter(|anchor| anchor.behind_doc) {
        compose_anchor(&mut list, anchor);
    }
    // Column separator rules (`w:cols/@w:sep`): a thin vertical hairline centered in
    // each inter-column gap, painted under the text layer (the gap carries no
    // glyphs, so z-order is immaterial).
    for sep in &page.separators {
        list.push(PaintItem::Line {
            from: Point::new(sep.x, sep.top),
            to: Point::new(sep.x, sep.bottom),
            stroke: Stroke {
                color: Color::BLACK,
                width: stroke_px(COLUMN_SEPARATOR_WIDTH),
            },
        });
    }
    for placed in &page.header {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for placed in &page.placed {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for placed in &page.footnotes {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for placed in &page.footer {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for anchor in floats.iter().filter(|anchor| !anchor.behind_doc) {
        compose_anchor(&mut list, anchor);
    }
    list
}

/// Paints one placed float at its resolved rectangle: an image blit, a filled/
/// stroked rectangle, a line/connector, or a text box (fill + border + its flowed
/// content, offset into the box by the internal margin, exactly like an inline
/// text box).
fn compose_anchor(list: &mut DisplayList, anchor: &PlacedAnchor) {
    match &anchor.content {
        AnchorContent::Image { media } => {
            list.push(PaintItem::Image {
                media: media.clone(),
                rect: anchor.rect,
            });
        }
        AnchorContent::Rectangle { fill, stroke } => {
            list.push(PaintItem::Rect {
                rect: anchor.rect,
                fill: fill.map(rgba),
                stroke: stroke.map(|s| Stroke {
                    color: rgba(s.color),
                    width: stroke_px(s.width),
                }),
            });
        }
        AnchorContent::Line { from, to, stroke } => {
            list.push(PaintItem::Line {
                from: *from,
                to: *to,
                stroke: Stroke {
                    color: rgba(stroke.color),
                    width: stroke_px(stroke.width),
                },
            });
        }
        AnchorContent::TextBox {
            blocks,
            fill,
            border,
            content_layout,
        } => {
            if let Some(fill) = fill {
                list.push(PaintItem::Rect {
                    rect: anchor.rect,
                    fill: Some(rgba(*fill)),
                    stroke: None,
                });
            }
            if let Some(border) = border {
                list.push(PaintItem::Rect {
                    rect: anchor.rect,
                    fill: None,
                    stroke: Some(Stroke {
                        color: rgba(border.color),
                        width: stroke_px(border.width),
                    }),
                });
            }
            let content_origin = Point::new(
                anchor.rect.origin.x + content_layout.origin.x,
                anchor.rect.origin.y + content_layout.origin.y,
            );
            let clip = text_box_clip(anchor.rect, *content_layout);
            if let Some(clip) = clip {
                list.push(PaintItem::PushClip(clip));
            }
            compose_blocks(list, blocks, content_origin);
            if clip.is_some() {
                list.push(PaintItem::PopClip);
            }
        }
    }
}

/// Builds a clip rectangle for the selected overflow axes. The unselected axis
/// receives a deliberately broad page-independent span; any enclosing table/page
/// clip still intersects it in the renderer.
fn text_box_clip(rect: Rect, layout: TextBoxContentLayout) -> Option<Rect> {
    if !layout.clip_horizontal && !layout.clip_vertical {
        return None;
    }
    const PAD: i32 = 1_000_000;
    let (x, width) = if layout.clip_horizontal {
        (rect.origin.x, rect.size.width)
    } else {
        (
            Twip(rect.origin.x.raw().saturating_sub(PAD)),
            Twip(rect.size.width.raw().saturating_add(PAD.saturating_mul(2))),
        )
    };
    let (y, height) = if layout.clip_vertical {
        (rect.origin.y, rect.size.height)
    } else {
        (
            Twip(rect.origin.y.raw().saturating_sub(PAD)),
            Twip(rect.size.height.raw().saturating_add(PAD.saturating_mul(2))),
        )
    };
    Some(Rect::new(Point::new(x, y), Size::new(width, height)))
}

/// Composes one block fragment at `origin` (top-left, twips) into `list`.
fn compose_fragment(list: &mut DisplayList, fragment: &BlockFragment, origin: Point) {
    match fragment {
        BlockFragment::Paragraph {
            lines,
            box_metrics,
            decor,
            ..
        } => {
            // The leading indent (`w:ind@start`) shifts the whole paragraph's
            // content origin to the indented column; the shaper already wrapped its
            // lines to the reduced width, and the first-line indent is baked into
            // the first line's run origins.
            let content_origin = Point::new(
                origin.x + box_metrics.indent_start,
                origin.y + box_metrics.space_before,
            );
            // Background shading + borders are painted first, behind the text.
            if !decor.is_empty() {
                let box_width = Twip(
                    (decor.width.raw()
                        - box_metrics.indent_start.raw()
                        - box_metrics.indent_end.raw())
                    .max(0),
                );
                let box_rect = Rect::new(content_origin, Size::new(box_width, lines.height()));
                compose_paragraph_decor(list, box_rect, decor);
            }
            list.items
                .extend(compose_paragraph(lines, content_origin).items);
        }
        BlockFragment::TableRow { cells, clip, .. } => {
            let row_height = fragment.height();
            for cell in cells {
                if matches!(cell.vertical_merge, CellVerticalMerge::Continue) {
                    continue;
                }
                let cell_height = cell.box_height(row_height);
                let cell_origin = Point::new(origin.x + cell.x, origin.y);
                let cell_rect = Rect::new(cell_origin, Size::new(cell.width, cell_height));
                // Cell background shading (`w:shd`) fills the cell before anything
                // else, behind both the grid line and the content.
                if let Some(fill) = cell.shading {
                    list.push(PaintItem::Rect {
                        rect: cell_rect,
                        fill: Some(rgba(fill)),
                        stroke: None,
                    });
                }
                // Only the cell's RESOLVED borders are drawn — no default grid
                // line. Word draws nothing for a border-less cell (common in
                // layout tables); a gray default grid would show boundaries Word
                // hides. Bordered tables get their borders via the resolved edges.
                compose_cell_borders(list, cell_rect, &cell.borders);
                // Content is inset by the cell margins (`w:tcMar`/`w:tblCellMar`)
                // and shifted down by the vertical-alignment slack (`w:vAlign`), so
                // it no longer hugs the top-left grid line; bottom-aligned labels
                // sit on the answer line. The shading/border/clip rects still span
                // the whole cell — only the flowed content moves.
                let content_origin = Point::new(
                    cell_origin.x + cell.margins.start,
                    cell_origin.y + cell.content_y_offset(cell_height),
                );
                // An `exact` row height clips content that overflows the cell.
                if *clip {
                    list.push(PaintItem::PushClip(cell_rect));
                    compose_blocks(list, &cell.blocks, content_origin);
                    list.push(PaintItem::PopClip);
                } else {
                    compose_blocks(list, &cell.blocks, content_origin);
                }
            }
        }
    }
}

/// Maximum number of on-runs expanded from one dashed edge. A pathological
/// sub-twip pattern falls back to one solid band instead of growing the display
/// list without bound.
const MAX_BORDER_PATTERN_RECTS: usize = 2_048;

#[derive(Clone, Copy)]
enum BorderAxis {
    Horizontal,
    Vertical,
}

/// Paints a cell's resolved (border-conflict-winning) edges. Horizontal sides
/// may be independently resolved at abutting grid boundaries.
fn compose_cell_borders(list: &mut DisplayList, rect: Rect, borders: &CellBorders) {
    compose_horizontal_border(list, rect, borders.top, &borders.top_segments, false);
    compose_horizontal_border(list, rect, borders.bottom, &borders.bottom_segments, true);
    if let Some(e) = borders.start {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(e.width, rect.size.height)),
            e,
            BorderAxis::Vertical,
        );
    }
    if let Some(e) = borders.end {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.right() - e.width, rect.origin.y),
                Size::new(e.width, rect.size.height),
            ),
            e,
            BorderAxis::Vertical,
        );
    }
}

fn compose_horizontal_border(
    list: &mut DisplayList,
    rect: Rect,
    fallback: Option<ResolvedEdge>,
    segments: &[ResolvedBorderSegment],
    bottom: bool,
) {
    if segments.is_empty() {
        let Some(edge) = fallback else {
            return;
        };
        let y = if bottom {
            rect.bottom() - edge.width
        } else {
            rect.origin.y
        };
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x, y),
                Size::new(rect.size.width, edge.width),
            ),
            edge,
            BorderAxis::Horizontal,
        );
        return;
    }
    for segment in segments {
        let y = if bottom {
            rect.bottom() - segment.edge.width
        } else {
            rect.origin.y
        };
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x + segment.offset, y),
                Size::new(segment.length, segment.edge.width),
            ),
            segment.edge,
            BorderAxis::Horizontal,
        );
    }
}

fn paint_border(list: &mut DisplayList, rect: Rect, edge: ResolvedEdge, axis: BorderAxis) {
    match edge.pattern {
        BorderPattern::Solid => push_border_rect(list, rect, edge.color),
        BorderPattern::Double => {
            let thickness = match axis {
                BorderAxis::Horizontal => rect.size.height,
                BorderAxis::Vertical => rect.size.width,
            };
            let band = Twip((thickness.raw() / 3).max(1));
            match axis {
                BorderAxis::Horizontal => {
                    push_border_rect(
                        list,
                        Rect::new(rect.origin, Size::new(rect.size.width, band)),
                        edge.color,
                    );
                    push_border_rect(
                        list,
                        Rect::new(
                            Point::new(rect.origin.x, rect.bottom() - band),
                            Size::new(rect.size.width, band),
                        ),
                        edge.color,
                    );
                }
                BorderAxis::Vertical => {
                    push_border_rect(
                        list,
                        Rect::new(rect.origin, Size::new(band, rect.size.height)),
                        edge.color,
                    );
                    push_border_rect(
                        list,
                        Rect::new(
                            Point::new(rect.right() - band, rect.origin.y),
                            Size::new(band, rect.size.height),
                        ),
                        edge.color,
                    );
                }
            }
        }
        pattern => {
            if let Some(rects) = patterned_rects(rect, edge.width, pattern, axis) {
                for dash in rects {
                    push_border_rect(list, dash, edge.color);
                }
            } else {
                push_border_rect(list, rect, edge.color);
            }
        }
    }
}

fn patterned_rects(
    rect: Rect,
    width: Twip,
    pattern: BorderPattern,
    axis: BorderAxis,
) -> Option<Vec<Rect>> {
    let runs: &[(i32, i32)] = match pattern {
        BorderPattern::Dotted => &[(1, 1)],
        BorderPattern::Dashed => &[(3, 2)],
        BorderPattern::DotDash => &[(1, 1), (3, 1)],
        BorderPattern::DotDotDash => &[(1, 1), (1, 1), (3, 1)],
        BorderPattern::Solid | BorderPattern::Double => return Some(vec![rect]),
    };
    let start = match axis {
        BorderAxis::Horizontal => rect.origin.x.raw(),
        BorderAxis::Vertical => rect.origin.y.raw(),
    };
    let total = match axis {
        BorderAxis::Horizontal => rect.size.width.raw(),
        BorderAxis::Vertical => rect.size.height.raw(),
    }
    .max(0);
    let end = start.saturating_add(total);
    let unit = width.raw().max(1);
    let cycle_units = runs
        .iter()
        .map(|(on, off)| on.saturating_add(*off))
        .sum::<i32>();
    let cycle = unit.saturating_mul(cycle_units).max(1);
    let mut out = Vec::new();
    let mut cursor = start - start.rem_euclid(cycle);
    let mut run_index = 0usize;
    while cursor < end {
        let (on_units, off_units) = runs[run_index % runs.len()];
        let on = unit.saturating_mul(on_units);
        let paint_start = cursor.max(start);
        let paint_end = cursor.saturating_add(on).min(end);
        if paint_start < paint_end {
            if out.len() == MAX_BORDER_PATTERN_RECTS {
                return None;
            }
            let dash = match axis {
                BorderAxis::Horizontal => Rect::new(
                    Point::new(Twip(paint_start), rect.origin.y),
                    Size::new(Twip(paint_end - paint_start), rect.size.height),
                ),
                BorderAxis::Vertical => Rect::new(
                    Point::new(rect.origin.x, Twip(paint_start)),
                    Size::new(rect.size.width, Twip(paint_end - paint_start)),
                ),
            };
            out.push(dash);
        }
        cursor = cursor.saturating_add(on);
        cursor = cursor.saturating_add(unit.saturating_mul(off_units));
        run_index += 1;
    }
    Some(out)
}

fn push_border_rect(list: &mut DisplayList, rect: Rect, color: [u8; 4]) {
    list.push(PaintItem::Rect {
        rect,
        fill: Some(Color {
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        }),
        stroke: None,
    });
}

/// Paints a paragraph's background shading (`w:shd`) as a fill covering `rect`
/// and its borders (`w:pBdr`) as filled edge rects, both behind the text.
fn compose_paragraph_decor(list: &mut DisplayList, rect: Rect, decor: &ParagraphDecor) {
    if let Some(fill) = decor.shading {
        list.push(PaintItem::Rect {
            rect,
            fill: Some(rgba(fill)),
            stroke: None,
        });
    }
    let b = &decor.borders;
    if let Some(e) = b.top {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(rect.size.width, e.width)),
            e,
            BorderAxis::Horizontal,
        );
    }
    if let Some(e) = b.bottom {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.origin.x, rect.bottom() - e.width),
                Size::new(rect.size.width, e.width),
            ),
            e,
            BorderAxis::Horizontal,
        );
    }
    if let Some(e) = b.start {
        paint_border(
            list,
            Rect::new(rect.origin, Size::new(e.width, rect.size.height)),
            e,
            BorderAxis::Vertical,
        );
    }
    if let Some(e) = b.end {
        paint_border(
            list,
            Rect::new(
                Point::new(rect.right() - e.width, rect.origin.y),
                Size::new(e.width, rect.size.height),
            ),
            e,
            BorderAxis::Vertical,
        );
    }
}

/// Composes a vertical stack of block fragments (a table cell's content) starting
/// at `origin`, advancing by each fragment's height.
fn compose_blocks(list: &mut DisplayList, blocks: &[BlockFragment], origin: Point) {
    let mut y = origin.y;
    for block in blocks {
        compose_fragment(list, block, Point::new(origin.x, y));
        y = y + block.height();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelPos, ModelRange};
    use crate::shape::ParleyShaper;
    use crate::text::{Decoration, FontId, LineConstraints, LineShaper, StyledRun};
    use crate::units::Twip;
    use casual_doc_model::NodeId;

    #[test]
    fn compose_places_glyph_runs_at_the_paragraph_origin() {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(1, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Hi".into(),
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
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let origin = Point::new(Twip::from_points(72), Twip::from_points(72));
        let list = compose_paragraph(&layout, origin);
        assert!(
            !list.items.is_empty(),
            "the paragraph composes to paint items"
        );
        let PaintItem::Glyphs { run } = &list.items[0] else {
            panic!("expected a glyph run");
        };
        // The run is translated by the paragraph origin (x at least the left margin).
        assert!(run.origin.x.raw() >= origin.x.raw());
        assert!(run.origin.y.raw() >= origin.y.raw());
    }

    #[test]
    fn an_exact_height_line_clips_oversized_glyph_ink_to_its_line_box() {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(2, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Oversized".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(24),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                line_exact: Some(Twip(120)),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 9)),
        );
        assert_eq!(layout.lines[0].height, Twip(120));
        assert!(layout.lines[0].clip);

        let origin = Point::new(Twip(50), Twip(75));
        let list = compose_paragraph(&layout, origin);
        let PaintItem::PushClip(clip) = &list.items[0] else {
            panic!("an exact line starts a vertical clip");
        };
        assert_eq!(clip.origin.y, origin.y);
        assert_eq!(clip.size.height, Twip(120));
        assert!(matches!(list.items.last(), Some(PaintItem::PopClip)));
    }

    #[test]
    fn an_exact_row_clips_and_draws_its_resolved_borders() {
        use crate::block::{
            BlockFragment, CellBorders, CellContentMargins, CellFragment, CellVAlign,
            CellVerticalMerge, ResolvedEdge,
        };
        use crate::units::Size;
        let cell = CellFragment {
            id: NodeId::from_parts(1, 1).unwrap(),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders {
                bottom: Some(ResolvedEdge {
                    color: [10, 20, 30, 255],
                    width: Twip(40),
                    pattern: BorderPattern::Solid,
                }),
                ..CellBorders::default()
            },
            shading: None,
        };
        let row = BlockFragment::TableRow {
            id: NodeId::from_parts(2, 1).unwrap(),
            table: NodeId::from_parts(3, 1).unwrap(),
            cells: vec![cell],
            height: Twip(300),
            can_split: false,
            header: false,
            merge_keep_next: false,
            clip: true,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &row, Point::new(Twip(100), Twip(200)));

        // The exact-height row wraps its content in a clip.
        assert!(
            list.items
                .iter()
                .any(|i| matches!(i, PaintItem::PushClip(_))),
            "an exact row pushes a clip"
        );
        assert!(
            list.items.iter().any(|i| matches!(i, PaintItem::PopClip)),
            "and pops it"
        );
        // The resolved bottom border is painted as a filled edge rect at the row
        // foot, 40 twips tall, in the resolved color.
        let border = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(color),
                stroke: None,
            } if rect.size == Size::new(Twip(3000), Twip(40)) => Some(*color),
            _ => None,
        });
        let color = border.expect("the resolved bottom border is drawn");
        assert_eq!(color.r, 10);
        assert_eq!(color.g, 20);
        assert_eq!(color.b, 30);
    }

    #[test]
    fn common_border_patterns_expand_to_deterministic_geometry() {
        let rect = Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(Twip(100), Twip(10)),
        );
        let cases = [
            (
                BorderPattern::Dotted,
                vec![(0, 10), (20, 10), (40, 10), (60, 10), (80, 10)],
            ),
            (BorderPattern::Dashed, vec![(0, 30), (50, 30)]),
            (
                BorderPattern::DotDash,
                vec![(0, 10), (20, 30), (60, 10), (80, 20)],
            ),
            (
                BorderPattern::DotDotDash,
                vec![(0, 10), (20, 10), (40, 30), (80, 10)],
            ),
        ];
        for (pattern, expected) in cases {
            let mut list = DisplayList::new();
            paint_border(
                &mut list,
                rect,
                ResolvedEdge {
                    color: [1, 2, 3, 255],
                    width: Twip(10),
                    pattern,
                },
                BorderAxis::Horizontal,
            );
            let actual: Vec<(i32, i32)> = list
                .items
                .iter()
                .filter_map(|item| match item {
                    PaintItem::Rect { rect, .. } => {
                        Some((rect.origin.x.raw(), rect.size.width.raw()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(actual, expected, "{pattern:?} keeps a stable phase");
        }
    }

    #[test]
    fn shared_dashes_keep_the_same_phase_across_different_segment_partitions() {
        let make_rect = |x, width| {
            Rect::new(
                Point::new(Twip(x), Twip::ZERO),
                Size::new(Twip(width), Twip(10)),
            )
        };
        let covered = |rects: Vec<Rect>| {
            rects
                .into_iter()
                .flat_map(|rect| rect.origin.x.raw()..rect.right().raw())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let whole = covered(
            patterned_rects(
                make_rect(13, 100),
                Twip(10),
                BorderPattern::DotDash,
                BorderAxis::Horizontal,
            )
            .unwrap(),
        );
        let partitioned = covered(
            [
                patterned_rects(
                    make_rect(13, 37),
                    Twip(10),
                    BorderPattern::DotDash,
                    BorderAxis::Horizontal,
                )
                .unwrap(),
                patterned_rects(
                    make_rect(50, 63),
                    Twip(10),
                    BorderPattern::DotDash,
                    BorderAxis::Horizontal,
                )
                .unwrap(),
            ]
            .concat(),
        );
        assert_eq!(
            whole, partitioned,
            "both cells sharing an edge paint the same on/off twips"
        );
    }

    #[test]
    fn a_double_border_paints_two_parallel_bands_inside_the_total_width() {
        let mut list = DisplayList::new();
        paint_border(
            &mut list,
            Rect::new(
                Point::new(Twip(100), Twip(200)),
                Size::new(Twip(500), Twip(60)),
            ),
            ResolvedEdge {
                color: [1, 2, 3, 255],
                width: Twip(60),
                pattern: BorderPattern::Double,
            },
            BorderAxis::Horizontal,
        );
        let bands: Vec<(i32, i32)> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect { rect, .. } => Some((rect.origin.y.raw(), rect.size.height.raw())),
                _ => None,
            })
            .collect();
        assert_eq!(bands, vec![(200, 20), (240, 20)]);
    }

    #[test]
    fn pathological_dash_expansion_falls_back_to_one_bounded_solid_band() {
        let rect = Rect::new(
            Point::new(Twip::ZERO, Twip::ZERO),
            Size::new(Twip(10_000), Twip(1)),
        );
        let mut list = DisplayList::new();
        paint_border(
            &mut list,
            rect,
            ResolvedEdge {
                color: [1, 2, 3, 255],
                width: Twip(1),
                pattern: BorderPattern::Dotted,
            },
            BorderAxis::Horizontal,
        );
        assert_eq!(list.items.len(), 1);
        assert!(matches!(
            list.items[0],
            PaintItem::Rect {
                rect: painted,
                ..
            } if painted == rect
        ));
    }

    #[test]
    fn horizontal_border_segments_override_the_whole_side_fallback() {
        let mut list = DisplayList::new();
        let borders = CellBorders {
            bottom: Some(ResolvedEdge {
                color: [255, 0, 255, 255],
                width: Twip(5),
                pattern: BorderPattern::Solid,
            }),
            bottom_segments: vec![
                ResolvedBorderSegment {
                    offset: Twip(0),
                    length: Twip(50),
                    edge: ResolvedEdge {
                        color: [255, 0, 0, 255],
                        width: Twip(10),
                        pattern: BorderPattern::Solid,
                    },
                },
                ResolvedBorderSegment {
                    offset: Twip(50),
                    length: Twip(50),
                    edge: ResolvedEdge {
                        color: [0, 0, 255, 255],
                        width: Twip(10),
                        pattern: BorderPattern::Solid,
                    },
                },
            ],
            ..CellBorders::default()
        };
        compose_cell_borders(
            &mut list,
            Rect::new(
                Point::new(Twip(20), Twip(30)),
                Size::new(Twip(100), Twip(40)),
            ),
            &borders,
        );
        let painted: Vec<(i32, i32, Color)> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect {
                    rect,
                    fill: Some(color),
                    ..
                } => Some((rect.origin.x.raw(), rect.size.width.raw(), *color)),
                _ => None,
            })
            .collect();
        assert_eq!(
            painted,
            vec![
                (20, 50, Color::rgb(255, 0, 0)),
                (70, 50, Color::rgb(0, 0, 255)),
            ],
            "segment geometry and colors paint independently; magenta fallback is absent"
        );
    }

    use crate::block::{BlockBorders, BoxMetrics, ParagraphDecor, ResolvedEdge};
    use crate::text::{Glyph, GlyphRun, Line, LineBreak, LineLayout};

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    /// A one-line, one-run paragraph fragment whose single glyph run sits at the
    /// paragraph-relative `origin` with the given `advance` and optional highlight.
    fn one_run_line(advance: Twip, highlight: Option<[u8; 4]>) -> LineLayout {
        let run = GlyphRun {
            font: FontId(0),
            size: Twip(200),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip(200)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight,
            glyphs: vec![Glyph {
                id: 1,
                advance,
                cluster: 0,
            }],
        };
        LineLayout {
            lines: vec![Line {
                runs: vec![run],
                ascent: Twip(200),
                descent: Twip(50),
                height: Twip(250),
                clip: false,
                range: ModelRange::new(ModelPos::new(node(1), 0), ModelPos::new(node(1), 0)),
                line_break: LineBreak::ParagraphEnd,
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                notes: Vec::new(),
                text_boxes: Vec::new(),
                rules: Vec::new(),
            }],
        }
    }

    #[test]
    fn a_bar_tab_stop_draws_a_vertical_rule() {
        // A line carrying a bar-stop x (1500) composes a thin vertical rule at that
        // x spanning the line height, at the paragraph origin.
        let mut layout = one_run_line(Twip(500), None);
        layout.lines[0].bars = vec![Twip(1500)];
        let list = compose_paragraph(&layout, Point::new(Twip(100), Twip(200)));
        let bar = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(fill),
                stroke: None,
            } if fill.r == 0 && rect.size.width == BAR_TAB_WIDTH => Some(rect.origin.x.raw()),
            _ => None,
        });
        assert_eq!(
            bar,
            Some(100 + 1500),
            "the bar rule is drawn at the paragraph origin plus the stop x"
        );
    }

    #[test]
    fn a_start_indent_shifts_the_composed_run_right() {
        // A paragraph with a 720-twip start indent composes its glyph runs at the
        // page origin plus the indent (the shaper already wrapped to the reduced
        // width; composition only applies the horizontal offset).
        let frag = BlockFragment::Paragraph {
            id: node(1),
            lines: one_run_line(Twip(500), None),
            box_metrics: BoxMetrics {
                indent_start: Twip(720),
                ..BoxMetrics::default()
            },
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor::default(),
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &frag, Point::new(Twip(1000), Twip(2000)));
        let glyph_x = list.items.iter().find_map(|i| match i {
            PaintItem::Glyphs { run } => Some(run.origin.x.raw()),
            _ => None,
        });
        assert_eq!(
            glyph_x,
            Some(1000 + 720),
            "the run starts at the page origin plus the start indent"
        );
    }

    #[test]
    fn a_highlighted_run_emits_a_fill_rect_behind_the_glyphs() {
        let layout = one_run_line(Twip(600), Some([255, 255, 0, 255]));
        let list = compose_paragraph(&layout, Point::new(Twip(100), Twip(200)));
        // The highlight fill precedes the glyph run.
        let fill_idx = list.items.iter().position(|i| {
            matches!(
                i,
                PaintItem::Rect {
                    fill: Some(c),
                    stroke: None,
                    ..
                } if *c == Color { r: 255, g: 255, b: 0, a: 255 }
            )
        });
        let glyph_idx = list
            .items
            .iter()
            .position(|i| matches!(i, PaintItem::Glyphs { .. }));
        let fill_idx = fill_idx.expect("a highlight fill rect is emitted");
        assert!(
            fill_idx < glyph_idx.expect("the glyphs are emitted"),
            "the highlight is painted behind (before) the glyphs"
        );
        // The fill spans the run's advance and the line's height.
        let PaintItem::Rect { rect, .. } = &list.items[fill_idx] else {
            unreachable!()
        };
        assert_eq!(rect.size.width, Twip(600));
        assert_eq!(rect.size.height, Twip(250));
    }

    #[test]
    fn a_shaded_paragraph_emits_a_background_rect_covering_its_box() {
        let frag = BlockFragment::Paragraph {
            id: node(1),
            lines: one_run_line(Twip(500), None),
            box_metrics: BoxMetrics::default(),
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor {
                shading: Some([220, 230, 240, 255]),
                borders: BlockBorders::default(),
                width: Twip(6000),
            },
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &frag, Point::new(Twip(0), Twip(0)));
        let shade = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(c),
                stroke: None,
            } if *c
                == (Color {
                    r: 220,
                    g: 230,
                    b: 240,
                    a: 255,
                }) =>
            {
                Some(*rect)
            }
            _ => None,
        });
        let rect = shade.expect("the shaded paragraph emits a background fill");
        assert_eq!(rect.size.width, Twip(6000), "the fill spans the box width");
        assert_eq!(rect.size.height, Twip(250), "and the lines' height");
    }

    #[test]
    fn a_bordered_paragraph_emits_border_edge_rects() {
        let edge = ResolvedEdge {
            color: [10, 20, 30, 255],
            width: Twip(40),
            pattern: BorderPattern::Solid,
        };
        let frag = BlockFragment::Paragraph {
            id: node(1),
            lines: one_run_line(Twip(500), None),
            box_metrics: BoxMetrics::default(),
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor {
                shading: None,
                borders: BlockBorders {
                    top: Some(edge),
                    bottom: Some(edge),
                    start: Some(edge),
                    end: Some(edge),
                },
                width: Twip(6000),
            },
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &frag, Point::new(Twip(0), Twip(0)));
        let border_rects = list
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    PaintItem::Rect {
                        fill: Some(c),
                        stroke: None,
                        ..
                    } if *c == (Color { r: 10, g: 20, b: 30, a: 255 })
                )
            })
            .count();
        assert_eq!(
            border_rects, 4,
            "all four paragraph border edges are stroked"
        );
    }

    #[test]
    fn a_shaded_cell_emits_a_fill_behind_its_content() {
        use crate::block::{
            CellBorders, CellContentMargins, CellFragment, CellVAlign, CellVerticalMerge,
        };
        let cell = CellFragment {
            id: node(1),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders::default(),
            shading: Some([200, 100, 50, 255]),
        };
        let row = BlockFragment::TableRow {
            id: node(2),
            table: node(3),
            cells: vec![cell],
            height: Twip(500),
            can_split: true,
            header: false,
            merge_keep_next: false,
            clip: false,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &row, Point::new(Twip(100), Twip(200)));
        // The very first paint op for the cell is its shading fill, behind the grid
        // line and content.
        let first_fill = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(c),
                stroke: None,
            } => Some((*rect, *c)),
            _ => None,
        });
        let (rect, color) = first_fill.expect("the shaded cell emits a fill");
        assert_eq!(
            color,
            Color {
                r: 200,
                g: 100,
                b: 50,
                a: 255
            }
        );
        assert_eq!(rect.origin, Point::new(Twip(100), Twip(200)));
        assert_eq!(rect.size, Size::new(Twip(3000), Twip(500)));
    }

    #[test]
    fn a_vertical_merge_paints_one_box_and_skips_its_continuation() {
        use crate::block::{
            CellBorders, CellContentMargins, CellFragment, CellVAlign, CellVerticalMerge,
        };
        let cell = |id, vertical_merge, shading| CellFragment {
            id: node(id),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge,
            borders: CellBorders::default(),
            shading,
        };
        let restart = BlockFragment::TableRow {
            id: node(10),
            table: node(20),
            cells: vec![cell(
                11,
                CellVerticalMerge::Restart { height: Twip(1000) },
                Some([200, 100, 50, 255]),
            )],
            height: Twip(500),
            can_split: true,
            header: false,
            merge_keep_next: true,
            clip: false,
        };
        let continuation = BlockFragment::TableRow {
            id: node(12),
            table: node(20),
            cells: vec![cell(
                13,
                CellVerticalMerge::Continue,
                Some([10, 20, 30, 255]),
            )],
            height: Twip(500),
            can_split: true,
            header: false,
            merge_keep_next: false,
            clip: false,
        };
        let mut list = DisplayList::new();
        compose_fragment(&mut list, &restart, Point::new(Twip(100), Twip(200)));
        compose_fragment(&mut list, &continuation, Point::new(Twip(100), Twip(700)));

        let fills: Vec<_> = list
            .items
            .iter()
            .filter_map(|item| match item {
                PaintItem::Rect {
                    rect,
                    fill: Some(fill),
                    ..
                } => Some((*rect, *fill)),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 1, "the continuation emits no duplicate box");
        assert_eq!(fills[0].0.size.height, Twip(1000));
        assert_eq!(
            fills[0].1,
            Color {
                r: 200,
                g: 100,
                b: 50,
                a: 255
            }
        );
    }

    #[test]
    fn an_inline_text_box_paints_its_fill_border_and_content() {
        use crate::text::{InlineTextBox, TextBoxContentLayout, TextBoxStroke};

        // A text box carrying one inner paragraph fragment (a single glyph run),
        // with an explicit fill and border.
        let inner = BlockFragment::Paragraph {
            id: node(2),
            lines: one_run_line(Twip(400), None),
            box_metrics: BoxMetrics::default(),
            break_control: crate::block::BreakControl::default(),
            decor: ParagraphDecor::default(),
        };
        let text_box = InlineTextBox {
            origin: Point::new(Twip(500), Twip(600)),
            size: Size::new(Twip(3000), Twip(1000)),
            blocks: vec![inner],
            border: Some(TextBoxStroke {
                color: [10, 20, 30, 255],
                width: Twip(30),
            }),
            fill: Some([200, 210, 220, 255]),
            content_layout: TextBoxContentLayout {
                origin: Point::new(Twip(72), Twip(72)),
                clip_horizontal: false,
                clip_vertical: false,
            },
        };
        let mut layout = one_run_line(Twip(0), None);
        layout.lines[0].runs.clear();
        layout.lines[0].text_boxes = vec![text_box];

        let origin = Point::new(Twip(100), Twip(200));
        let list = compose_paragraph(&layout, origin);

        // The fill covers the box at its page-absolute origin.
        let fill = list.items.iter().find_map(|i| match i {
            PaintItem::Rect {
                rect,
                fill: Some(c),
                stroke: None,
            } if *c
                == (Color {
                    r: 200,
                    g: 210,
                    b: 220,
                    a: 255,
                }) =>
            {
                Some(*rect)
            }
            _ => None,
        });
        let fill_rect = fill.expect("the box fill paints");
        assert_eq!(fill_rect.origin, Point::new(Twip(600), Twip(800)));
        assert_eq!(fill_rect.size, Size::new(Twip(3000), Twip(1000)));

        // The border paints as a stroked rect over the same box.
        assert!(
            list.items.iter().any(|i| matches!(
                i,
                PaintItem::Rect {
                    stroke: Some(s),
                    fill: None,
                    ..
                } if s.color == (Color { r: 10, g: 20, b: 30, a: 255 })
                    && (s.width - 2.0).abs() < f32::EPSILON
            )),
            "the box border paints as a stroked rect"
        );

        // The inner glyph run composes offset into the box by the internal margin.
        let glyph_x = list.items.iter().find_map(|i| match i {
            PaintItem::Glyphs { run } => Some(run.origin.x.raw()),
            _ => None,
        });
        assert_eq!(
            glyph_x,
            Some(672),
            "the box content is inset from the box's left edge by the margin"
        );
    }
}
