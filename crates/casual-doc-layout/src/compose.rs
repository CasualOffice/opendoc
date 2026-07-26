//! Composition — turning a shaped [`LineLayout`] into a [`DisplayList`].
//!
//! This is the seam between layout and rendering: each shaped glyph run is
//! translated to its position on the page and emitted as a paint item. The list
//! stays in device-independent twips (consistent with the whole engine); the
//! rendering backend applies the device scale (DPI × zoom) when it paints, which
//! is the "scale only at paint" rule from `43-…`.

use crate::block::{BlockFragment, CellBorders};
use crate::display::{Color, DisplayList, PaintItem, Stroke};
use crate::page::Page;
use crate::text::LineLayout;
use crate::units::{Point, Rect, Size};

/// Table cell grid-line color and width.
const CELL_BORDER: Color = Color::rgb(160, 160, 160);
const CELL_BORDER_WIDTH: f32 = 1.0;

/// Builds a display list for one paragraph's shaped lines, placed with the
/// paragraph's top-left at `origin` (in twips). The shaper positions each glyph
/// run relative to the paragraph's own origin (run `origin` = the run's left edge
/// on its baseline); composition translates those into page coordinates.
#[must_use]
pub fn compose_paragraph(layout: &LineLayout, origin: Point) -> DisplayList {
    let mut list = DisplayList::new();
    for line in &layout.lines {
        for run in &line.runs {
            let mut placed = run.clone();
            placed.origin = Point::new(origin.x + run.origin.x, origin.y + run.origin.y);
            list.push(PaintItem::Glyphs { run: placed });
        }
    }
    list
}

/// Builds the display list for a whole paginated [`Page`]: each placed fragment
/// (paragraph or table row) is composed at its position on the page.
#[must_use]
pub fn compose_page(page: &Page) -> DisplayList {
    let mut list = DisplayList::new();
    for placed in &page.placed {
        compose_fragment(&mut list, &placed.fragment, placed.rect.origin);
    }
    list
}

/// Composes one block fragment at `origin` (top-left, twips) into `list`.
fn compose_fragment(list: &mut DisplayList, fragment: &BlockFragment, origin: Point) {
    match fragment {
        BlockFragment::Paragraph {
            lines, box_metrics, ..
        } => {
            let content_origin = Point::new(origin.x, origin.y + box_metrics.space_before);
            list.items
                .extend(compose_paragraph(lines, content_origin).items);
        }
        BlockFragment::TableRow { cells, clip, .. } => {
            let row_height = fragment.height();
            for cell in cells {
                let cell_origin = Point::new(origin.x + cell.x, origin.y);
                let cell_rect = Rect::new(cell_origin, Size::new(cell.width, row_height));
                // The default grid line (drawn behind the content); resolved
                // conflict-winning borders are painted over it per edge.
                list.push(PaintItem::Rect {
                    rect: cell_rect,
                    fill: None,
                    stroke: Some(Stroke {
                        color: CELL_BORDER,
                        width: CELL_BORDER_WIDTH,
                    }),
                });
                compose_cell_borders(list, cell_rect, &cell.borders);
                // An `exact` row height clips content that overflows the cell.
                if *clip {
                    list.push(PaintItem::PushClip(cell_rect));
                    compose_blocks(list, &cell.blocks, cell_origin);
                    list.push(PaintItem::PopClip);
                } else {
                    compose_blocks(list, &cell.blocks, cell_origin);
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
        use crate::block::{BlockFragment, CellBorders, CellFragment, ResolvedEdge};
        use crate::units::Size;
        let cell = CellFragment {
            id: NodeId::from_parts(1, 1).unwrap(),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            blocks: Vec::new(),
            borders: CellBorders {
                bottom: Some(ResolvedEdge {
                    color: [10, 20, 30, 255],
                    width: Twip(40),
                }),
                ..CellBorders::default()
            },
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
}
