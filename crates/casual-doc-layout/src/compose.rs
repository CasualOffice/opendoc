//! Composition — turning a shaped [`LineLayout`] into a [`DisplayList`].
//!
//! This is the seam between layout and rendering: each shaped glyph run is
//! translated to its position on the page and emitted as a paint item. The list
//! stays in device-independent twips (consistent with the whole engine); the
//! rendering backend applies the device scale (DPI × zoom) when it paints, which
//! is the "scale only at paint" rule from `43-…`.

use crate::block::{BlockFragment, CellBorders, ParagraphDecor};
use crate::display::{Color, DisplayList, PaintItem, Stroke};
use crate::page::{AnchorContent, Page, PlacedAnchor};
use crate::text::LineLayout;
use crate::units::{Point, Rect, Size, Twip};

/// Width (twips) of a `bar` tab stop's vertical rule (~0.5pt, Word's hairline).
const BAR_TAB_WIDTH: Twip = Twip(10);

/// The internal margin (twips) between an inline text box's border and its flowed
/// content — Word's default `wps:bodyPr` inset (~0.05in). The flow layer sizes the
/// box to include it on all four sides ([`crate::flow`]); composition offsets the
/// content by it.
pub(crate) const TEXTBOX_INSET: Twip = Twip(72);

/// Stroke width (device px) of an inline text box's border (a hairline).
const TEXTBOX_BORDER_WIDTH: f32 = 1.0;

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
                        color: rgba(border),
                        width: TEXTBOX_BORDER_WIDTH,
                    }),
                });
            }
            let content_origin =
                Point::new(box_origin.x + TEXTBOX_INSET, box_origin.y + TEXTBOX_INSET);
            compose_blocks(&mut list, &text_box.blocks, content_origin);
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
    for placed in &page.header {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    for placed in &page.placed {
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
                        color: rgba(*border),
                        width: TEXTBOX_BORDER_WIDTH,
                    }),
                });
            }
            let content_origin = Point::new(
                anchor.rect.origin.x + TEXTBOX_INSET,
                anchor.rect.origin.y + TEXTBOX_INSET,
            );
            compose_blocks(list, blocks, content_origin);
        }
    }
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
                let cell_origin = Point::new(origin.x + cell.x, origin.y);
                let cell_rect = Rect::new(cell_origin, Size::new(cell.width, row_height));
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
                    cell_origin.y + cell.content_y_offset(row_height),
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

/// Paints a cell's resolved (border-conflict-winning) edges as filled rects, one
/// per present side, on top of the default grid line.
fn compose_cell_borders(list: &mut DisplayList, rect: Rect, borders: &CellBorders) {
    let mut edge = |r: Rect, color: [u8; 4]| {
        list.push(PaintItem::Rect {
            rect: r,
            fill: Some(Color {
                r: color[0],
                g: color[1],
                b: color[2],
                a: color[3],
            }),
            stroke: None,
        });
    };
    if let Some(e) = borders.top {
        edge(
            Rect::new(rect.origin, Size::new(rect.size.width, e.width)),
            e.color,
        );
    }
    if let Some(e) = borders.bottom {
        edge(
            Rect::new(
                Point::new(rect.origin.x, rect.bottom() - e.width),
                Size::new(rect.size.width, e.width),
            ),
            e.color,
        );
    }
    if let Some(e) = borders.start {
        edge(
            Rect::new(rect.origin, Size::new(e.width, rect.size.height)),
            e.color,
        );
    }
    if let Some(e) = borders.end {
        edge(
            Rect::new(
                Point::new(rect.right() - e.width, rect.origin.y),
                Size::new(e.width, rect.size.height),
            ),
            e.color,
        );
    }
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
    let mut edge = |r: Rect, color: [u8; 4]| {
        list.push(PaintItem::Rect {
            rect: r,
            fill: Some(rgba(color)),
            stroke: None,
        });
    };
    let b = &decor.borders;
    if let Some(e) = b.top {
        edge(
            Rect::new(rect.origin, Size::new(rect.size.width, e.width)),
            e.color,
        );
    }
    if let Some(e) = b.bottom {
        edge(
            Rect::new(
                Point::new(rect.origin.x, rect.bottom() - e.width),
                Size::new(rect.size.width, e.width),
            ),
            e.color,
        );
    }
    if let Some(e) = b.start {
        edge(
            Rect::new(rect.origin, Size::new(e.width, rect.size.height)),
            e.color,
        );
    }
    if let Some(e) = b.end {
        edge(
            Rect::new(
                Point::new(rect.right() - e.width, rect.origin.y),
                Size::new(e.width, rect.size.height),
            ),
            e.color,
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
                font: FontId(0),
                size: Twip::from_points(11),
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
    fn an_exact_row_clips_and_draws_its_resolved_borders() {
        use crate::block::{
            BlockFragment, CellBorders, CellContentMargins, CellFragment, CellVAlign, ResolvedEdge,
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
            borders: CellBorders {
                bottom: Some(ResolvedEdge {
                    color: [10, 20, 30, 255],
                    width: Twip(40),
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
                range: ModelRange::new(ModelPos::new(node(1), 0), ModelPos::new(node(1), 0)),
                line_break: LineBreak::ParagraphEnd,
                page_break_after: false,
                bars: Vec::new(),
                images: Vec::new(),
                fields: Vec::new(),
                text_boxes: Vec::new(),
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
        use crate::block::{CellBorders, CellContentMargins, CellFragment, CellVAlign};
        let cell = CellFragment {
            id: node(1),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            blocks: Vec::new(),
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
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
    fn an_inline_text_box_paints_its_fill_border_and_content() {
        use crate::text::InlineTextBox;

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
            border: Some([10, 20, 30, 255]),
            fill: Some([200, 210, 220, 255]),
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
            Some(600 + TEXTBOX_INSET.raw()),
            "the box content is inset from the box's left edge by the margin"
        );
    }
}
