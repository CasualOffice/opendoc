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
//! ([`shape_geometry_content`](casual_doc_layout::anchor::shape_geometry_content),
//! [`themed_shape_appearance`](casual_doc_layout::anchor::themed_shape_appearance),
//! [`GroupMapper`](casual_doc_layout::anchor::GroupMapper) and
//! [`GroupPose`](casual_doc_layout::anchor::GroupPose)), so a `prstGeom` cannot look
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

use casual_doc_layout::anchor::{GroupMapper, GroupPose, shape_geometry_content};
use casual_doc_layout::page::{AnchorContent, AnchorZ, PlacedAnchor};
use casual_doc_layout::units::{Point, Rect, Size, emu_to_twip_extent};
use casual_doc_model::v1::{Definitions, GroupChild, MAX_GROUP_DEPTH};
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
    let mut anchors = Vec::new();
    let mut order = 0_u32;
    let definitions = presentation.definitions();

    for tree in cascade_trees(presentation, slide) {
        place_tree(tree, definitions, size, &mut anchors, &mut order);
    }
    Some(SlideCanvas { size, anchors })
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

/// Places one shape tree's children onto the surface.
fn place_tree(
    tree: &ShapeTree,
    definitions: &Definitions,
    size: Size,
    out: &mut Vec<PlacedAnchor>,
    order: &mut u32,
) {
    // The tree's own transform maps its child space onto the surface. A `p:spTree`
    // normally declares a child space identical to the slide, so this is usually the
    // identity — but it is applied rather than assumed, because a deck authored at a
    // different `a:chExt` than its `p:sldSz` is legal and renders scaled.
    let mapper = GroupMapper::from_transform(&tree.transform);
    let origin = Point::new(
        casual_doc_layout::units::Twip(0),
        casual_doc_layout::units::Twip(0),
    );
    for child in &tree.children {
        if child.hidden {
            continue;
        }
        place_child(
            &child.content,
            definitions,
            origin,
            &mapper,
            GroupPose::IDENTITY,
            out,
            order,
            0,
        );
        let _ = size;
    }
}

/// Places one drawing child, recursing into nested groups.
#[allow(clippy::too_many_arguments)]
fn place_child(
    child: &GroupChild,
    definitions: &Definitions,
    origin: Point,
    mapper: &GroupMapper,
    pose: GroupPose,
    out: &mut Vec<PlacedAnchor>,
    order: &mut u32,
    depth: u32,
) {
    match child {
        GroupChild::Shape(shape) => {
            let rect = pose.reposition(mapper.child_rect(origin, shape.offset, shape.extent));
            let (fill, stroke) =
                casual_doc_layout::anchor::themed_shape_appearance(shape, definitions);
            let content = shape_geometry_content(
                shape.geometry,
                shape.preset.as_deref(),
                &shape.adjustments,
                rect,
                fill.as_ref(),
                stroke,
            );
            let (rotation, flip_h, flip_v) =
                pose.compose_child(shape.rotation, shape.flip_h, shape.flip_v);
            push(
                out,
                order,
                Some(shape.id),
                content,
                rect,
                rotation,
                flip_h,
                flip_v,
            );
        }
        GroupChild::Picture(picture) => {
            let Some(media) = definitions.media.get(&picture.media) else {
                // A validated presentation cannot reach this; skipping rather than
                // painting a placeholder keeps a malformed deck from inventing content.
                return;
            };
            let rect = pose.reposition(mapper.child_rect(origin, picture.offset, picture.extent));
            let content = AnchorContent::Image {
                media: media.part_name.clone(),
                crop: picture.crop,
                border: casual_doc_layout::anchor::shape_stroke(picture.border),
                opacity: picture.opacity,
            };
            let (rotation, flip_h, flip_v) =
                pose.compose_child(picture.rotation, picture.flip_h, picture.flip_v);
            push(
                out,
                order,
                Some(picture.id),
                content,
                rect,
                rotation,
                flip_h,
                flip_v,
            );
        }
        GroupChild::Group(group) => {
            if depth + 1 > MAX_GROUP_DEPTH {
                return;
            }
            let nested_mapper = mapper.compose(group);
            let nested_box =
                mapper.child_rect(origin, group.transform.offset, group.transform.extent);
            let nested_pose = pose.after(GroupPose::about(
                rect_center(nested_box),
                group.transform.rotation.unwrap_or(0),
                group.transform.flip_h,
                group.transform.flip_v,
            ));
            for nested in &group.children {
                place_child(
                    nested,
                    definitions,
                    origin,
                    &nested_mapper,
                    nested_pose,
                    out,
                    order,
                    depth + 1,
                );
            }
        }
        // A slide's text lives in `SlideNode::text`, and `GroupChild::TextBox` is
        // refused on a slide by the model — so this arm is unreachable for a
        // validated presentation and paints nothing rather than guessing.
        GroupChild::TextBox(_) => {}
    }
}

/// The center of a rectangle, for the pose a nested group rotates about.
fn rect_center(rect: Rect) -> Point {
    Point::new(
        casual_doc_layout::units::Twip(rect.origin.x.raw() + rect.size.width.raw() / 2),
        casual_doc_layout::units::Twip(rect.origin.y.raw() + rect.size.height.raw() / 2),
    )
}

/// Appends a placed anchor, assigning the next paint-order key.
#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Vec<PlacedAnchor>,
    order: &mut u32,
    node: Option<casual_doc_model::NodeId>,
    content: AnchorContent,
    rect: Rect,
    rotation: Option<i32>,
    flip_h: bool,
    flip_v: bool,
) {
    out.push(PlacedAnchor {
        node,
        content,
        rect,
        behind_doc: false,
        z: AnchorZ {
            relative_height: 0,
            order: *order,
        },
        descr: None,
        transform: casual_doc_layout::anchor::shape_transform(rect, flip_h, flip_v, rotation),
    });
    *order = order.saturating_add(1);
}

#[cfg(test)]
mod tests;
