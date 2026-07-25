//! Composition — turning a shaped [`LineLayout`] into a [`DisplayList`].
//!
//! This is the seam between layout and rendering: each shaped glyph run is
//! translated to its position on the page and emitted as a paint item. The list
//! stays in device-independent twips (consistent with the whole engine); the
//! rendering backend applies the device scale (DPI × zoom) when it paints, which
//! is the "scale only at paint" rule from `43-…`.

use crate::block::BlockFragment;
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
        BlockFragment::TableRow { cells, .. } => {
            let row_height = fragment.height();
            for cell in cells {
                let cell_origin = Point::new(origin.x + cell.x, origin.y);
                // The cell's grid border (drawn behind the content).
                list.push(PaintItem::Rect {
                    rect: Rect::new(cell_origin, Size::new(cell.width, row_height)),
                    fill: None,
                    stroke: Some(Stroke {
                        color: CELL_BORDER,
                        width: CELL_BORDER_WIDTH,
                    }),
                });
                compose_blocks(list, &cell.blocks, cell_origin);
            }
        }
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
                text: "Hi",
                font: FontId(0),
                size: Twip::from_points(11),
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
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
}
