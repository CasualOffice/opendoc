// SPDX-License-Identifier: Apache-2.0

//! Slide layout: a [`Presentation`] onto the backend-neutral display list
//! `casual-doc-layout` already defines.
//!
//! # Why this is small, and why that is the finding
//!
//! A document's layout is the hard part of this engine: shaping, line breaking, the
//! style cascade, float avoidance, and pagination. **A slide has none of it.** The
//! surface is a fixed size, every shape states its own rectangle, and nothing flows,
//! so laying out a slide is a coordinate mapping and a paint order — which is exactly
//! what `docs/156` §8 predicted when it called slide layout "small; consumes §3.2's
//! published seams".
//!
//! So this crate deliberately contains no geometry, no theme resolution and no preset
//! evaluation. All three come from `casual_doc_layout::anchor`'s published seams
//! ([`casual_doc_layout::anchor::shape_geometry_content`],
//! [`casual_doc_layout::anchor::themed_shape_appearance`],
//! [`GroupMapper`] and
//! [`GroupPose`]), so a `prstGeom` cannot look
//! one way in a document and another on a slide.
//!
//! # What this does NOT do
//!
//! **It does not lay out text.** A slide's `a:txBody` is modeled
//! ([`casual_pres_model::TextBody`]) and is deliberately not shaped here: slide text
//! needs the placeholder inheritance cascade resolved through
//! `a:lstStyle` → layout → master → `p:defaultTextStyle` before a single run has a
//! font size, and that cascade is not built. Emitting unshaped text would put
//! something on screen that is not what the file says, which is worse than a shape
//! with no text in it. Shapes, pictures, fills, outlines, preset geometry, groups and
//! the three-tier paint order do render.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use casual_doc_layout::anchor::{
    GroupChildHost, GroupMapper, GroupPose, anchor_shadow, place_group_child_tree,
};
use casual_doc_layout::display::ShapeTransform;
use casual_doc_layout::page::{AnchorContent, AnchorZ, PlacedAnchor};
use casual_doc_layout::units::{Point, Rect, Size, Twip, emu_to_twip_extent};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Definitions, GroupChild};
use casual_pres_model::{Presentation, ShapeTree, Slide};

/// One slide, resolved to a surface size and a paint-ordered display list.
#[derive(Clone, Debug)]
pub struct SlideCanvas {
    /// The surface every slide shares (`p:sldSz`), in twips.
    pub size: Size,
    /// What to paint, in paint order: the master's shapes, then the layout's, then
    /// the slide's own.
    pub anchors: Vec<PlacedAnchor>,
}

/// Composes a laid-out slide into the backend-neutral display list.
///
/// The same `DisplayList` a DOCX page produces, which is the point: the `tiny-skia`
/// raster backend, the PDF writer and the hit-tester all consume it already, so a
/// slide reaches a screen and a PDF through code that has no notion of slides.
///
/// # Complexity
///
/// O(n log n) in this slide's anchors, from the paint-order sort.
#[must_use]
pub fn compose_slide(canvas: &SlideCanvas) -> casual_doc_layout::display::DisplayList {
    casual_doc_layout::compose::compose_anchors(&canvas.anchors)
}

/// Lays out the slide at `index` in presentation order.
///
/// Returns `None` when `index` is past the end, which is the only failure mode: a
/// validated [`Presentation`] has resolvable layout and master references, so the
/// cascade cannot dangle here.
///
/// # Paint order, which is the whole of the cascade this crate implements
///
/// Master first, then layout, then slide. That order IS the inheritance for painted
/// background furniture — a slide's own shape paints over the layout's placeholder
/// outline, which paints over the master's background — and it is why this is three
/// passes rather than a merge. Within one tree, a child's index is its z-order, as in
/// a DOCX group.
///
/// # Complexity
///
/// O(shapes on this slide + its layout + its master). Independent of the deck's
/// length: nothing here walks other slides, which is what keeps opening a 300-slide
/// deck from being quadratic.
#[must_use]
pub fn lay_out_slide(presentation: &Presentation, index: usize) -> Option<SlideCanvas> {
    let slide = presentation.slides().get(index)?;
    let surface = presentation.slide_size();
    let size = Size::new(
        emu_to_twip_extent(surface.width_emu),
        emu_to_twip_extent(surface.height_emu),
    );
    let mut host = SlideHost {
        definitions: presentation.definitions(),
        anchors: Vec::new(),
        order: 0,
    };
    let origin = Point::new(Twip(0), Twip(0));
    for tree in cascade_trees(presentation, slide) {
        // The tree's own transform maps its child space onto the surface. A
        // `p:spTree` normally declares a child space equal to `p:sldSz`, so this is
        // usually the identity — but it is applied rather than assumed, because a
        // deck authored at a different `a:chExt` is legal and renders scaled.
        let mapper = GroupMapper::from_transform(&tree.transform);
        let visible: Vec<GroupChild> = tree
            .children
            .iter()
            .filter(|child| !child.hidden)
            .map(|child| child.content.clone())
            .collect();
        place_group_child_tree(&visible, origin, &mapper, GroupPose::IDENTITY, 0, &mut host);
    }
    Some(SlideCanvas {
        size,
        anchors: host.anchors,
    })
}

/// The three trees that paint a slide, in paint order.
///
/// A missing layout or master yields a shorter list rather than nothing: a slide with
/// an unresolvable layout is refused by validation, so this cannot happen for a
/// validated presentation — but painting the slide's own shapes is a better failure
/// than painting an empty surface if it ever does.
fn cascade_trees<'a>(presentation: &'a Presentation, slide: &'a Slide) -> Vec<&'a ShapeTree> {
    let layout = presentation.layout_of(slide);
    let master = layout.and_then(|layout| presentation.master_of(layout));
    let mut trees = Vec::with_capacity(3);
    if let Some(master) = master {
        trees.push(&master.shapes);
    }
    if let Some(layout) = layout {
        trees.push(&layout.shapes);
    }
    trees.push(&slide.shapes);
    trees
}

/// The slide's host for the shared `GroupChild` walk: a flat float layer over the
/// surface, with one monotonic paint order.
///
/// Implementing [`GroupChildHost`] rather than writing a second recursion is the
/// whole point. A slide and a DOCX float paint the same vocabulary, so the mapper,
/// the pose, the preset table, the theme matrix AND the custom-geometry path all come
/// from one walk — this crate's first draft had its own copy and silently omitted
/// `GroupShape::path`, so a slide carrying an `a:custGeom` painted its bounding preset
/// instead of its authored outline.
struct SlideHost<'a> {
    definitions: &'a Definitions,
    anchors: Vec<PlacedAnchor>,
    order: u32,
}

impl GroupChildHost for SlideHost<'_> {
    fn definitions(&self) -> &Definitions {
        self.definitions
    }

    fn emit(
        &mut self,
        node: NodeId,
        content: AnchorContent,
        rect: Rect,
        descr: Option<String>,
        transform: Option<ShapeTransform>,
    ) {
        self.anchors.push(PlacedAnchor {
            node: Some(node),
            content,
            rect,
            // A slide has no text layer, so nothing can be behind the document.
            behind_doc: false,
            z: AnchorZ {
                // A slide carries no `wp:anchor@relativeHeight`; call order alone
                // decides, which is what makes the three-tier cascade a paint order.
                relative_height: 0,
                order: self.order,
            },
            descr,
            transform,
            // From the same resolver the document host uses, so a shape authored with
            // `a:outerShdw` casts the identical shadow on a slide and in a DOCX.
            shadow: anchor_shadow(self.definitions, node),
        });
        self.order = self.order.saturating_add(1);
    }

    // `emit_text_box` is deliberately NOT overridden. Slide text is `a:txBody` on the
    // shape (`SlideNode::text`) and the model refuses `GroupChild::TextBox` on a
    // slide, so the default no-op is unreachable rather than unimplemented — and the
    // trait's default documents that, instead of this crate silently dropping an arm.
}

#[cfg(test)]
mod tests;
