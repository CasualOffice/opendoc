//! Composition — turning a shaped [`LineLayout`] into a [`DisplayList`].
//!
//! This is the seam between layout and rendering: each shaped glyph run is
//! translated to its position on the page and emitted as a paint item. The list
//! stays in device-independent twips (consistent with the whole engine); the
//! rendering backend applies the device scale (DPI × zoom) when it paints, which
//! is the "scale only at paint" rule from `43-…`.

use crate::display::{DisplayList, PaintItem};
use crate::text::LineLayout;
use crate::units::Point;

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
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                rtl: false,
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
