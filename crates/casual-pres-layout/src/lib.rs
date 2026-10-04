// SPDX-License-Identifier: Apache-2.0

//! Slide layout: a [`Presentation`] onto the backend-neutral display list
//! `casual-doc-layout` already defines.
//!
//! # Why this is small, and why that is the finding
//!
//! A document's layout is the hard part of this engine: shaping, line breaking, the
//! style cascade, float avoidance, and pagination. **A slide has none of the last
//! three.** The surface is a fixed size, every shape states (or inherits) its own
//! rectangle, and nothing flows, so placing a slide is a coordinate mapping and a
//! paint order — which is exactly what `docs/156` §8 predicted when it called slide
//! layout "small; consumes §3.2's published seams".
//!
//! So this crate deliberately contains no geometry, no theme resolution and no
//! preset evaluation. All three come from `casual_doc_layout::anchor`'s published
//! seams ([`casual_doc_layout::anchor::shape_geometry_content`],
//! [`casual_doc_layout::anchor::themed_shape_appearance`],
//! [`GroupMapper`] and
//! [`GroupPose`]), so a `prstGeom` cannot look
//! one way in a document and another on a slide. It contains no shaper either:
//! slide text goes through the same [`LineShaper`] a DOCX text box does — see
//! [`text`].
//!
//! # What this does NOT do
//!
//! * **Bullets, tab stops, vertical text and an `a:normAutofit` re-solve.** Each
//!   is enumerated in [`text`]'s own documentation, with the reason.
//! * **A chart or SmartArt frame's CONTENT.** A `p:graphicFrame` is placed — a
//!   table reaches the display list through [`AnchorContent::Table`], the same
//!   arm a positioned DOCX table uses, and `table`'s own documentation carries
//!   the argument — but a `c:chart` or a `dgm:relIds` payload never arrives in
//!   the model, so its frame is a positioned empty box. `docs/156` §8 leaves both
//!   to `docs/155`/ADR-050 and the importer reports each.
//! * **A table STYLE.** The `a:tableStyleId` GUID resolves to a `TableStyles`
//!   entry that carries an id and a name and no formatting, so a styled table
//!   paints its cells' own fills and borders and nothing the style adds. Also
//!   `a:tblPr@rtl`: the grid is painted left to right whatever it says, because
//!   mirroring the grid without mirroring each cell's `a:lnL`/`a:lnR` and
//!   `@marL`/`@marR` is worse than not mirroring at all.
//! * **Grouped text.** A text-bearing `p:sp` *inside* a `p:grpSp` carries no text
//!   into the model at all — `GroupChild` has nowhere to put an `a:txBody` — so it
//!   cannot reach here. The importer reports it as `grpSp/txBody`, and it is the
//!   sharpest remaining gap in slide text.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use casual_doc_layout::anchor::{
    GroupChildHost, GroupMapper, GroupPose, anchor_shadow, place_group_child_tree,
};
use casual_doc_layout::display::ShapeTransform;
use casual_doc_layout::page::{AnchorContent, AnchorShadow, AnchorZ, PlacedAnchor};
use casual_doc_layout::text::LineShaper;
use casual_doc_layout::units::{Point, Rect, Size, Twip, emu_to_twip_extent};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Definitions, GroupChild};
use casual_pres_model::{PlaceholderKind, Presentation, ShapeTree, Slide, SlideNode, SlideTable};

pub mod outline;
mod table;
pub mod text;

pub use outline::{
    OutlineParagraph, OutlineShape, SlideTextOutline, SlideTextRole, SlideTextTier,
    slide_text_outline,
};
pub use text::{LAST_RESORT_COLOR, LAST_RESORT_SIZE, UnresolvedProperty, UnresolvedTextProperty};

/// One slide, resolved to a surface size and a paint-ordered display list.
///
/// `#[non_exhaustive]` so a later field — a hit-test index, a loss count — is not
/// a breaking change to every literal that builds one (`SKILL` §5a shape 5).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SlideCanvas {
    /// The surface every slide shares (`p:sldSz`), in twips.
    pub size: Size,
    /// What to paint, in paint order: the master's shapes, then the layout's, then
    /// the slide's own. A shape's text follows the shape itself, so the text of an
    /// earlier shape can never paint over a later one.
    pub anchors: Vec<PlacedAnchor>,
    /// Every text property this slide states that the engine could not resolve.
    ///
    /// Carried out rather than logged: the caller is the only layer that can
    /// decide whether an unresolved theme font is a warning, a fidelity row, or
    /// nothing at all. Empty for a slide whose text resolves completely.
    pub unresolved: Vec<UnresolvedTextProperty>,
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

/// Lays out the slide at `index` in presentation order, shaping its text with
/// `shaper`.
///
/// Returns `None` when `index` is past the end, which is the only failure mode: a
/// validated [`Presentation`] has resolvable layout and master references, so the
/// cascade cannot dangle here.
///
/// # Why the shaper is a parameter
///
/// The same reason the document engine takes one: shaping is the seam behind which
/// the concrete text stack sits, and a deterministic configuration
/// (`ParleyShaper::without_system_fonts`) must be selectable by the caller — a
/// guard written against a shaper that can see the host's installed faces passes
/// on a developer's machine and proves nothing about a browser's bundle.
///
/// # Paint order, which is the whole of the cascade this crate implements
///
/// Master first, then layout, then slide. That order IS the inheritance for painted
/// background furniture — a slide's own shape paints over the layout's placeholder
/// outline, which paints over the master's background — and it is why this is three
/// passes rather than a merge. Within one tree, a child's index is its z-order, as in
/// a DOCX group, and a shape's own text is emitted immediately after the shape.
///
/// # Complexity
///
/// O(shapes on this slide + its layout + its master), plus the shaper's cost per
/// text-bearing shape. Independent of the deck's length: nothing here walks other
/// slides, which is what keeps opening a 300-slide deck from being quadratic.
#[must_use]
pub fn lay_out_slide(
    presentation: &Presentation,
    index: usize,
    shaper: &dyn LineShaper,
) -> Option<SlideCanvas> {
    let slide = presentation.slides().get(index)?;
    let surface = presentation.slide_size();
    let size = Size::new(
        emu_to_twip_extent(surface.width_emu),
        emu_to_twip_extent(surface.height_emu),
    );
    let mut host = SlideHost {
        definitions: presentation.definitions(),
        presentation,
        slide,
        anchors: Vec::new(),
        order: 0,
        shaper,
        texts: Vec::new(),
        tables: Vec::new(),
        unresolved: Vec::new(),
    };
    let origin = Point::new(Twip(0), Twip(0));
    for tier in cascade_tiers(presentation, slide) {
        // The tree's own transform maps its child space onto the surface. A
        // `p:spTree` normally declares a child space equal to `p:sldSz`, so this is
        // usually the identity — but it is applied rather than assumed, because a
        // deck authored at a different `a:chExt` is legal and renders scaled.
        let mapper = GroupMapper::from_transform(&tier.tree.transform);
        let mut visible: Vec<GroupChild> = Vec::with_capacity(tier.tree.children.len());
        host.texts.clear();
        host.tables.clear();
        for child in tier.tree.children.iter().filter(|child| !child.hidden) {
            visible.push(inherited_geometry(child, &tier.fallbacks));
            if tier.paints_text(child)
                && let Some(prepared) = text::prepare(presentation, slide, child)
            {
                host.texts.push((child.id(), prepared));
            }
            // A table is carried the same way a text body is, and for the same
            // reason: the walk is what knows the frame's rectangle, so the flow
            // has to happen inside `emit` rather than here.
            if let Some(table) = child.table.as_ref() {
                host.tables.push((child.id(), child, table));
            }
        }
        place_group_child_tree(&visible, origin, &mapper, GroupPose::IDENTITY, 0, &mut host);
    }
    Some(SlideCanvas {
        size,
        anchors: host.anchors,
        unresolved: host.unresolved,
    })
}

/// One tier of the three that paint a slide: its tree, and the trees a
/// placeholder on it inherits geometry from.
struct CascadeTier<'a> {
    /// The tree being painted.
    tree: &'a ShapeTree,
    /// The trees below it in the inheritance chain, nearest first.
    fallbacks: Vec<&'a ShapeTree>,
    /// Whether a shape in a placeholder slot on this tier paints its own text.
    ///
    /// `false` for the layout and the master, and that is the one rule slide text
    /// has that nothing else does: the `a:txBody` of a placeholder on a layout or
    /// a master is the **prompt** ("Click to edit Master title style"), which
    /// PowerPoint shows in the editor and never paints on a slide or in print. A
    /// non-placeholder shape on either tier — a logo caption, a running footer
    /// rule's label — is ordinary content and does paint, which is why this is a
    /// property of the slot and not of the tier alone.
    placeholder_text_paints: bool,
}

impl CascadeTier<'_> {
    /// Whether `child`'s text, if it has any, paints on the slide.
    fn paints_text(&self, child: &SlideNode) -> bool {
        self.placeholder_text_paints || child.placeholder.is_none()
    }
}

/// The three tiers that paint a slide, in paint order, each with its geometry
/// fallbacks.
///
/// A missing layout or master yields a shorter list rather than nothing: a slide
/// with an unresolvable layout is refused by validation, so this cannot happen for
/// a validated presentation — but painting the slide's own shapes is a better
/// failure than painting an empty surface if it ever does.
fn cascade_tiers<'a>(presentation: &'a Presentation, slide: &'a Slide) -> Vec<CascadeTier<'a>> {
    let layout = presentation.layout_of(slide);
    let master = layout.and_then(|layout| presentation.master_of(layout));
    let mut tiers = Vec::with_capacity(3);
    if let Some(master) = master {
        tiers.push(CascadeTier {
            tree: &master.shapes,
            fallbacks: Vec::new(),
            placeholder_text_paints: false,
        });
    }
    if let Some(layout) = layout {
        tiers.push(CascadeTier {
            tree: &layout.shapes,
            fallbacks: master
                .map(|master| vec![&master.shapes])
                .unwrap_or_default(),
            placeholder_text_paints: false,
        });
    }
    let mut fallbacks = Vec::with_capacity(2);
    if let Some(layout) = layout {
        fallbacks.push(&layout.shapes);
    }
    if let Some(master) = master {
        fallbacks.push(&master.shapes);
    }
    tiers.push(CascadeTier {
        tree: &slide.shapes,
        fallbacks,
        placeholder_text_paints: true,
    });
    tiers
}

/// A placeholder shape's drawing, with the box it inherits substituted in when it
/// states none of its own.
///
/// # Why this is here at all
///
/// `casual-pres-import` deliberately does **not** materialise inherited geometry
/// into the shape: a title on a real slide carries `<p:spPr/>` and nothing else, so
/// writing the layout's rectangle into it would turn inheritance into authorship
/// and a round trip would persist it. The importer's own header says the cascade is
/// "left to the consumer", and this crate is the consumer. Without this, every
/// placeholder on every real deck is a zero-sized box and its text has no width to
/// wrap in.
///
/// The condition is ONLYOFFICE's, quoted in that same header: inherit only when the
/// shape's own transform record is empty. A zero `a:ext` is the model's spelling of
/// that, because `v1::GroupShape` stores `offset`/`extent` as plain values and so
/// cannot distinguish an absent `a:xfrm` from an explicit zero one — a cost the
/// importer names rather than hides, and the one place it bites.
///
/// Substituted into the drawing handed to the **shared** walk, rather than resolved
/// separately for the text, so a shape and its glyphs cannot disagree about where
/// the shape is. A group is never substituted: its box and its child space
/// (`a:chOff`/`a:chExt`) are one transform, and moving one without the other would
/// misplace every child.
///
/// Only a `p:sp` inherits here. A `p:pic` filling a picture placeholder keeps its
/// own box, which is what real files carry: PowerPoint writes a full `a:xfrm` onto
/// an inserted picture because the crop and the aspect ratio it chose are not
/// derivable from the slot. A `p:pic` with no `a:xfrm` therefore stays a point, and
/// that is a stated limit rather than a resolved case.
///
/// # Complexity
///
/// O(shapes in the fallback trees), bounded by the shapes on one layout plus one
/// master, and independent of the deck's length.
fn inherited_geometry(node: &SlideNode, fallbacks: &[&ShapeTree]) -> GroupChild {
    let mut content = node.content.clone();
    let Some(placeholder) = node.placeholder else {
        return content;
    };
    let GroupChild::Shape(shape) = &mut content else {
        return content;
    };
    if shape.extent.width_emu != 0 && shape.extent.height_emu != 0 {
        return content;
    }
    for tree in fallbacks {
        let Some(source) = slot_in(tree, placeholder.kind, placeholder.index) else {
            continue;
        };
        let GroupChild::Shape(source) = &source.content else {
            continue;
        };
        if source.extent.width_emu != 0 && source.extent.height_emu != 0 {
            shape.offset = source.offset;
            shape.extent = source.extent;
            return content;
        }
    }
    content
}

/// The shape filling a slot on one tree, matching `title` and `ctrTitle` to each
/// other.
///
/// A plain `(kind, index)` match is not enough for the one slot that matters most:
/// PowerPoint writes `ctrTitle` on a title-slide layout and `title` on the master,
/// so a slide's `ctrTitle` placeholder matches its layout and then misses the
/// master entirely. `ShapeTree::title` already folds the two, and
/// `PlaceholderKind::is_title` is the model's own statement that they are one slot.
///
/// `casual_pres_model::Presentation::text_cascade` matches on the raw pair and so
/// misses a master's title `a:lstStyle` for a `ctrTitle` slide. That is a model-side
/// gap, not one this crate can close.
fn slot_in(tree: &ShapeTree, kind: PlaceholderKind, index: u32) -> Option<&SlideNode> {
    if kind.is_title() {
        return tree.title();
    }
    tree.slot(kind, index)
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
    /// The deck, for a table cell's text cascade — the one thing a cell needs
    /// that the walk does not supply.
    presentation: &'a Presentation,
    /// The slide being laid out, for the same reason.
    slide: &'a Slide,
    anchors: Vec<PlacedAnchor>,
    order: u32,
    /// The text stack this host shapes with.
    shaper: &'a dyn LineShaper,
    /// The current tier's text-bearing shapes, folded through the cascade and
    /// waiting for the walk to supply each one's rectangle.
    ///
    /// A `Vec` and not a map: a slide tree holds a dozen shapes and at most a
    /// handful carry text, so a linear scan beats building and hashing a map per
    /// tier — and the lookup happens once per emitted child, not once per glyph.
    texts: Vec<(NodeId, text::PreparedText<'a>)>,
    /// The current tier's `p:graphicFrame` tables, waiting for the walk to supply
    /// each frame's rectangle. A `Vec` for the same reason `texts` is one: a
    /// slide holds a handful of frames at most.
    tables: Vec<(NodeId, &'a SlideNode, &'a SlideTable)>,
    unresolved: Vec<UnresolvedTextProperty>,
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
        // A `p:graphicFrame` holding a table paints the TABLE and not the frame.
        // The frame's own `AnchorContent` is an unfilled, unstroked rectangle —
        // it is a position, not a drawing — so emitting it as well would put a
        // paint item in every deck's display list that draws nothing, and the
        // frame's rectangle is the only thing the walk was needed for.
        if let Some(position) = self.tables.iter().position(|(id, _, _)| *id == node) {
            let (_, frame, table) = self.tables.swap_remove(position);
            let mut unresolved = Vec::new();
            let flowed = table::flow(
                self.presentation,
                self.slide,
                frame,
                table,
                self.shaper,
                &mut unresolved,
            );
            self.unresolved.append(&mut unresolved);
            if let Some(flowed) = flowed {
                // The frame's ORIGIN with the TABLE's own size. A table's width is
                // the sum of its grid and its height the sum of its rows, and
                // PowerPoint re-derives the frame's `a:ext` from those rather than
                // the other way round — so a frame whose `a:ext` has gone stale
                // paints at the size its grid states, which is what PowerPoint
                // shows.
                let sized = Rect {
                    origin: rect.origin,
                    size: Size::new(table::width(table), table::authored_height(table)),
                };
                self.push(node, flowed, sized, descr, transform, None);
                return;
            }
        }
        // From the same resolver the document host uses, so a shape authored with
        // `a:outerShdw` casts the identical shadow on a slide and in a DOCX.
        let shadow = anchor_shadow(self.definitions, node);
        self.push(node, content, rect, descr, transform, shadow);
        // The shape's text, emitted from inside `emit` so it takes the very next
        // paint order after the shape it belongs to. Collecting the texts and
        // emitting them after the walk would let a later shape in the same tree
        // paint over an earlier shape's glyphs.
        let Some(position) = self.texts.iter().position(|(id, _)| *id == node) else {
            return;
        };
        let (_, prepared) = self.texts.swap_remove(position);
        let mut unresolved = Vec::new();
        let text = text::flow(&prepared, rect, self.shaper, &mut unresolved);
        self.unresolved.append(&mut unresolved);
        if let Some(text) = text {
            // The same node as the shape: the text IS the shape's, so a click that
            // resolves to one must resolve to the other. A `p:txBody` has no id of
            // its own in PPTX.
            //
            // NO shadow, although it carries the shape's node. `a:effectLst` on a
            // `p:spPr` is the SHAPE's effect; the text's own effects are a separate
            // `a:effectLst` inside `a:rPr`, which is not modelled. Re-resolving the
            // shape's shadow here would cast it a second time, from the glyphs,
            // which is not what the file says.
            self.push(node, text, rect, None, None, None);
        }
    }

    // `emit_text_box` is deliberately NOT overridden. Slide text is `a:txBody` on the
    // shape (`SlideNode::text`) and the model refuses `GroupChild::TextBox` on a
    // slide, so the default no-op is unreachable rather than unimplemented — and the
    // trait's default documents that, instead of this crate silently dropping an arm.
}

impl SlideHost<'_> {
    /// Appends one anchor at the next paint order.
    fn push(
        &mut self,
        node: NodeId,
        content: AnchorContent,
        rect: Rect,
        descr: Option<String>,
        transform: Option<ShapeTransform>,
        shadow: Option<AnchorShadow>,
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
            shadow,
        });
        self.order = self.order.saturating_add(1);
    }
}

#[cfg(test)]
mod tests;
