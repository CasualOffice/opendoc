// SPDX-License-Identifier: Apache-2.0

//! A slide's shape tree (`p:spTree`).
//!
//! # What is deliberately reused
//!
//! The children are `casual_doc_model::v1::GroupChild` — the **same** DrawingML
//! vocabulary a DOCX group holds, unchanged. That is not an economy, it is the
//! finding the design rests on (`docs/156` §3): `p:sp`, `p:pic` and `p:grpSp` are
//! `a:xfrm` + `a:prstGeom`/`a:custGeom` + `a:solidFill`/`a:gradFill` + `a:ln`, which
//! is exactly what `GroupShape`/`GroupPicture` already model and what the preset
//! table, the guide evaluator and the display list already paint. Declaring a
//! parallel `SlideShape` would fork the geometry engine on day one, and the two
//! copies would disagree by the second preset.
//!
//! # What is genuinely new
//!
//! [`SlideNode`] wraps each child with the three things a slide has and a document
//! does not: the [`Placeholder`] slot it inherits through, the author-visible name
//! the selection pane shows, and the hidden flag. These are a **wrapper**, not new
//! fields on `GroupShape`, and deliberately so — `GroupShape` has 23 literal
//! construction sites across six crates, so widening it is a breaking change to all
//! of them (`SKILL` §5a shape 1) for a field DOCX would never read.
//!
//! [`SlidePaint`] joins them for the same reason and closes a named gap: a slide
//! shape can state `<a:noFill/>`, and `v1::GroupShape::fill` is an `Option<Fill>`
//! in which `None` would have to mean both "states nothing, so inherit" and
//! "states nothing deliberately, so inherit nothing". A document has no
//! placeholder cascade and no theme style matrix behind an absent fill, so the
//! document model never needed the third state; a slide does.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::{
    Definitions, GroupChild, GroupTransform, MAX_GROUP_DEPTH, visit_group_child_node_ids,
};
use casual_doc_model::{ModelError, NodeId};
use serde::{Deserialize, Serialize};

use crate::{PaintProperty, Placeholder, PlaceholderKind, PresentationError, SlideTable, TextBody};

/// What a slide shape's `p:spPr` **states** about one paintable property — its
/// fill (`a:solidFill` and its siblings) or its outline (`a:ln`).
///
/// # Three states, because the file has three
///
/// | This value | The markup | What a consumer must do |
/// | --- | --- | --- |
/// | [`SlidePaint::Inherited`] | no fill element / no `a:ln` | inherit: the placeholder slot, then the shape's `p:style` theme reference, then the theme default |
/// | [`SlidePaint::Authored`] | `a:solidFill`, `a:gradFill`, `a:blipFill`, … / an `a:ln` with a line in it | paint what the drawing carries, and inherit nothing |
/// | [`SlidePaint::Suppressed`] | `<a:noFill/>` / `<a:ln><a:noFill/></a:ln>` | paint **nothing**, and inherit nothing |
///
/// # Why this is one enum and not a `bool`
///
/// A `bool` beside `Option<Fill>` makes four combinations of which one —
/// "explicitly nothing, and here is the something" — is not a state any file can
/// be in, and an unreachable state in a model is a branch every consumer has to
/// invent an answer for. The three real states are a three-valued enum, and
/// [`ShapeTree::validate`] refuses the pairing that would reintroduce the fourth
/// ([`PresentationError::SuppressedPaintCarriesValue`]).
///
/// # Why `Authored` is recorded although the value lives elsewhere
///
/// It is not redundant with `fill.is_some()`. A stated fill this build cannot
/// model — `a:gradFill`, `a:pattFill` — arrives as `Authored` with no value, and
/// that is exactly the case where inheriting would be wrong: the shape is filled
/// in the file, just not in a way this build can paint, so filling it from the
/// theme instead would substitute one wrong appearance for another and hide the
/// reported loss behind a plausible colour.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub enum SlidePaint {
    /// The shape states nothing, so the property is inherited.
    ///
    /// The default, because it is what the overwhelming majority of real shapes
    /// state and what a hand-built model means when it says nothing.
    #[default]
    Inherited,
    /// The shape states the property, and its value — when this build can model
    /// one — is on the drawing (`v1::GroupShape::fill`, `::stroke`,
    /// `v1::GroupPicture::border`).
    Authored,
    /// The shape states `<a:noFill/>`: deliberately nothing, inheriting nothing.
    Suppressed,
}

impl SlidePaint {
    /// Whether the shape stated nothing.
    ///
    /// Public because it is this type's `skip_serializing_if`: an absent field
    /// deserializes to [`SlidePaint::Inherited`], so writing it out would put the
    /// default in every node of every snapshot.
    #[must_use]
    pub const fn is_inherited(&self) -> bool {
        matches!(self, Self::Inherited)
    }

    /// Whether the shape stated `a:noFill` — the one state a painter must act on
    /// rather than fall through.
    #[must_use]
    pub const fn suppresses(&self) -> bool {
        matches!(self, Self::Suppressed)
    }
}

/// One shape on a slide, layout or master: a DrawingML child plus the slide-only
/// identity it carries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideNode {
    /// The slot this shape inherits position, size and text properties through
    /// (`p:nvSpPr/p:nvPr/p:ph`), or `None` for a shape that states everything
    /// itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<Placeholder>,
    /// The author-visible name (`p:cNvPr@name`) the selection pane shows, when the
    /// file carries a meaningful one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the shape is hidden (`p:cNvPr@hidden`). A hidden shape is retained
    /// rather than dropped: it must survive a round trip, and the author can unhide
    /// it.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hidden: bool,
    /// What the shape's `p:spPr` states about its FILL.
    ///
    /// The drawing holds the fill's value; this holds whether there was one to
    /// hold. See [`SlidePaint`] for why the third state cannot live on
    /// `v1::GroupShape`.
    #[serde(default, skip_serializing_if = "SlidePaint::is_inherited")]
    pub fill: SlidePaint,
    /// What the shape's `p:spPr` states about its OUTLINE (`a:ln`).
    ///
    /// Separate from `fill` and not folded with it, because a shape states the two
    /// independently and the common real case states them *differently*: an
    /// unfilled box with a visible border, and a filled box with none, are both
    /// ordinary.
    #[serde(default, skip_serializing_if = "SlidePaint::is_inherited")]
    pub outline: SlidePaint,
    /// The drawing itself, in the tree's child coordinate space.
    pub content: GroupChild,
    /// The shape's text (`p:txBody`), when it holds any.
    ///
    /// On the NODE rather than inside the drawing, because that is where PPTX puts
    /// it: a `p:sp` is `p:spPr` plus `p:txBody`, so text is a property of a shape
    /// rather than a distinct kind of child. Modeling it as a child would have meant
    /// reusing `GroupChild::TextBox`, whose `Vec<BlockNode>` cannot express an
    /// outline level, an inline bullet or an `a:lstStyle` — see
    /// [`crate::PresentationError::TextBoxShapeOnSlide`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextBody>,
    /// The table this node's `p:graphicFrame` holds (`a:graphicData/a:tbl`), when
    /// it holds one.
    ///
    /// On the NODE for the same reason `text` is, and the precedent is deliberate:
    /// a `p:graphicFrame` is a positioned, unpainted box plus a payload, and
    /// `GroupChild` has a field for the box and none for the payload. So the box
    /// imports as the `GroupChild::Shape` every other slide shape is — which is
    /// what lets the SHARED placement walk compute a frame's rectangle with no
    /// table arm anywhere in it — and the payload hangs here.
    ///
    /// A frame holding a `c:chart` or a `dgm:relIds` therefore arrives with this
    /// `None`: the frame's position, name and hidden flag are read, its payload is
    /// reported. That is a narrower loss than dropping the frame, and it is why
    /// `graphicFrame` is no longer in the loss report while `chart` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<SlideTable>,
}

impl SlideNode {
    /// A shape with no placeholder slot, no name, not hidden, and stating nothing
    /// about its fill or its outline.
    #[must_use]
    pub const fn new(content: GroupChild) -> Self {
        Self {
            placeholder: None,
            name: None,
            hidden: false,
            // `Inherited` and not "whatever the drawing carries": a caller that
            // builds a filled shape by hand has stated the fill on the drawing,
            // where every painter already reads it, so the only consumer this
            // choice can mislead is one asking "did the FILE say so" — and the
            // answer for a hand-built shape is no.
            fill: SlidePaint::Inherited,
            outline: SlidePaint::Inherited,
            content,
            text: None,
            table: None,
        }
    }

    /// Records what the shape stated about its fill.
    #[must_use]
    pub const fn stating_fill(mut self, fill: SlidePaint) -> Self {
        self.fill = fill;
        self
    }

    /// Records what the shape stated about its outline.
    #[must_use]
    pub const fn stating_outline(mut self, outline: SlidePaint) -> Self {
        self.outline = outline;
        self
    }

    /// Attaches a text body.
    #[must_use]
    pub fn with_text(mut self, text: TextBody) -> Self {
        self.text = Some(text);
        self
    }

    /// Attaches the `a:tbl` a `p:graphicFrame` carries.
    #[must_use]
    pub fn with_table(mut self, table: SlideTable) -> Self {
        self.table = Some(table);
        self
    }

    /// Attaches a placeholder slot.
    #[must_use]
    pub const fn in_slot(mut self, placeholder: Placeholder) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// This shape's own stable id, which is its drawing's id.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        match &self.content {
            GroupChild::Picture(picture) => picture.id,
            GroupChild::TextBox(text_box) => text_box.id,
            GroupChild::Shape(shape) => shape.id,
            GroupChild::Group(group) => group.id,
        }
    }
}

/// The ordered shape tree of a slide, layout or master (`p:spTree`).
///
/// Children are stored in **document order, which is paint order**: a later child
/// paints over an earlier one, exactly as in a DOCX group. There is no separate
/// z-index — the index *is* the z-index — which is why reordering is a list move and
/// not a renumbering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeTree {
    /// Stable identity (`p:spTree/p:nvGrpSpPr/p:cNvPr@id`).
    pub id: NodeId,
    /// The tree's transform (`p:grpSpPr/a:xfrm`): its box and its child coordinate
    /// space. For a top-level `p:spTree` the box is the slide surface and the child
    /// space is normally identical to it, so children are in slide EMU directly.
    pub transform: GroupTransform,
    /// The shapes, in paint order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SlideNode>,
}

impl ShapeTree {
    /// An empty tree whose child space matches its box, which is what a blank slide
    /// carries.
    #[must_use]
    pub fn empty(id: NodeId, width_emu: i64, height_emu: i64) -> Self {
        use casual_doc_model::v1::{Extent, PointEmu};
        let extent = Extent {
            width_emu,
            height_emu,
        };
        let origin = PointEmu { x_emu: 0, y_emu: 0 };
        Self {
            id,
            transform: GroupTransform {
                offset: origin,
                extent,
                child_offset: origin,
                child_extent: extent,
                flip_h: false,
                flip_v: false,
                rotation: None,
            },
            children: Vec::new(),
        }
    }

    /// Visits every node id this tree carries, its own included.
    ///
    /// Descends through the **shared** `casual-doc-model` traversal
    /// ([`visit_group_child_node_ids`]), so a slide and a document agree about what
    /// a drawing's ids are by construction rather than by two walks staying in
    /// step.
    ///
    /// # Complexity
    ///
    /// O(tree): one visit per node. Not for a keystroke.
    pub fn visit_node_ids(
        &self,
        visit: &mut dyn FnMut(NodeId) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        visit(self.id)?;
        for child in &self.children {
            visit_group_child_node_ids(&child.content, visit)?;
            // The text's own ids are in the same space, so a paragraph id colliding
            // with a shape id must be caught too.
            if let Some(text) = child.text.as_ref() {
                let mut failure = None;
                text.visit_node_ids(&mut |id| {
                    if failure.is_none() {
                        failure = visit(id).err();
                    }
                });
                if let Some(error) = failure {
                    return Err(error);
                }
            }
            // A table's rows, cells and cell text are in the SAME id space, so a
            // cell id colliding with a shape id has to be caught by the one
            // uniqueness walk rather than by a second check nobody runs.
            if let Some(table) = child.table.as_ref() {
                let mut failure = None;
                table.visit_node_ids(&mut |id| {
                    if failure.is_none() {
                        failure = visit(id).err();
                    }
                });
                if let Some(error) = failure {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Resolves a placeholder slot to the shape that fills it.
    ///
    /// This is the lookup the inheritance cascade runs: a slide shape's
    /// `(type, idx)` against its layout, then the layout's against its master.
    ///
    /// # Complexity
    ///
    /// O(children) — a linear scan of ONE tree, which is bounded by the shapes on a
    /// slide and independent of deck size. Deliberately not an index: building one
    /// per tree would cost more than scanning the dozen shapes a real slide holds,
    /// and a cache would have to be invalidated on every edit.
    #[must_use]
    pub fn slot(&self, kind: PlaceholderKind, index: u32) -> Option<&SlideNode> {
        // `title` and `ctrTitle` are ONE slot for inheritance, and matching the pair
        // literally is how that was got wrong: a slide's `ctrTitle` looked up
        // `(CtrTitle, idx)`, a master carries `title`, the lookup missed, and every
        // title slide inherited neither its geometry nor its text tiers. The two
        // tokens differ only in where the layout puts the box — a centred title is
        // still the title placeholder — which is why `PlaceholderKind::is_title`
        // exists and why `TextStyles::tier` already folded them.
        //
        // The index is NOT folded. A two-content layout has two `body` slots
        // distinguished only by `@idx`, so matching on the kind alone would pick the
        // wrong one half the time.
        self.children.iter().find(|child| {
            child.placeholder.is_some_and(|placeholder| {
                let (candidate, candidate_index) = placeholder.slot();
                candidate_index == index
                    && (candidate == kind || (candidate.is_title() && kind.is_title()))
            })
        })
    }

    /// The shape filling the title slot, matching either title token.
    ///
    /// # Complexity
    ///
    /// O(children), as [`ShapeTree::slot`].
    #[must_use]
    pub fn title(&self) -> Option<&SlideNode> {
        self.children.iter().find(|child| {
            child
                .placeholder
                .is_some_and(|placeholder| placeholder.kind.is_title())
        })
    }

    /// Checks this tree's own invariants: a usable child space, unique placeholder
    /// slots, at most one title, bounded group nesting, and resolvable media.
    ///
    /// Node-id uniqueness is **not** checked here — it is a property of the whole
    /// presentation, not of one tree, so [`crate::Presentation::validate`] owns it.
    ///
    /// # What this deliberately does not check
    ///
    /// A media reference reachable only through a text box's **block** content is
    /// not validated. Slide text is `a:txBody`, not `w:p`; the `BlockNode` carrier
    /// here is interim, so a check written against it would be discarded with it.
    /// Stated rather than left ambiguous (`SKILL` §8).
    ///
    /// # Complexity
    ///
    /// O(tree).
    pub fn validate(&self, definitions: &Definitions) -> Result<(), PresentationError> {
        if !self.children.is_empty()
            && (self.transform.child_extent.width_emu == 0
                || self.transform.child_extent.height_emu == 0)
        {
            return Err(PresentationError::DegenerateChildSpace(self.id));
        }
        self.validate_placeholders()?;
        for child in &self.children {
            if matches!(child.content, GroupChild::TextBox(_)) {
                return Err(PresentationError::TextBoxShapeOnSlide(child.id()));
            }
            if let Some(text) = child.text.as_ref() {
                text.validate()?;
            }
            if let Some(table) = child.table.as_ref() {
                table.validate()?;
            }
            validate_stated_paint(child)?;
            validate_child(&child.content, definitions, 0)?;
        }
        Ok(())
    }

    /// Enforces one shape per slot, and one title per tree.
    fn validate_placeholders(&self) -> Result<(), PresentationError> {
        let mut seen: BTreeSet<(PlaceholderKind, u32)> = BTreeSet::new();
        let mut titles = 0_u32;
        for child in &self.children {
            let Some(placeholder) = child.placeholder else {
                continue;
            };
            if !seen.insert(placeholder.slot()) {
                return Err(PresentationError::DuplicatePlaceholder {
                    tree: self.id,
                    kind: placeholder.kind,
                    index: placeholder.index,
                });
            }
            if placeholder.kind.is_title() {
                titles += 1;
                if titles > 1 {
                    return Err(PresentationError::DuplicateTitlePlaceholder(self.id));
                }
            }
        }
        Ok(())
    }

    /// Every slot this tree fills, in paint order, for the inheritance cascade and
    /// for guards that must derive a count rather than hand-maintain one.
    ///
    /// # Complexity
    ///
    /// O(children).
    #[must_use]
    pub fn slots(&self) -> BTreeMap<(PlaceholderKind, u32), NodeId> {
        self.children
            .iter()
            .filter_map(|child| {
                child
                    .placeholder
                    .map(|placeholder| (placeholder.slot(), child.id()))
            })
            .collect()
    }
}

/// Refuses the one combination of [`SlidePaint`] and drawing that no file can be
/// in: "explicitly nothing" beside a value.
///
/// This is what keeps the tri-state a tri-state. Without it the pair
/// ([`SlidePaint::Suppressed`], `Some(fill)`) is representable, and a painter
/// reaching it has to invent a rule — which is precisely the fourth, impossible
/// state a `bool` beside an `Option` would have admitted.
///
/// A picture's FILL is deliberately not checked: a `p:pic`'s image is its
/// `p:blipFill`, not its `p:spPr` fill, so `<a:noFill/>` on a picture's shape
/// properties says something about the box behind the image — which this model has
/// no field for — and is consistent with the picture having one.
///
/// # Complexity
///
/// O(1) per node.
fn validate_stated_paint(node: &SlideNode) -> Result<(), PresentationError> {
    let (fill, stroke) = match &node.content {
        GroupChild::Shape(shape) => (shape.fill.is_some(), shape.stroke.is_some()),
        GroupChild::Picture(picture) => (false, picture.border.is_some()),
        // Neither a group nor a text box carries a slide-stated fill or outline in
        // this model: `WordprocessingGroup` has no fill field at all, and a text
        // box on a slide is refused outright a few lines above this call.
        GroupChild::Group(_) | GroupChild::TextBox(_) => (false, false),
    };
    if node.fill.suppresses() && fill {
        return Err(PresentationError::SuppressedPaintCarriesValue {
            shape: node.id(),
            property: PaintProperty::Fill,
        });
    }
    if node.outline.suppresses() && stroke {
        return Err(PresentationError::SuppressedPaintCarriesValue {
            shape: node.id(),
            property: PaintProperty::Outline,
        });
    }
    Ok(())
}

/// Checks one drawing child's nesting bound and media references, recursing into
/// nested groups.
///
/// `depth` counts enclosing groups, so the top-level children of a `p:spTree` are at
/// zero. The `p:spTree` itself is not counted: it is the root container every slide
/// has, so counting it would spend one of the sixteen levels on nothing.
fn validate_child(
    child: &GroupChild,
    definitions: &Definitions,
    depth: u32,
) -> Result<(), PresentationError> {
    match child {
        GroupChild::Picture(picture) => {
            if !definitions.media.contains_key(&picture.media) {
                return Err(PresentationError::DanglingMediaRef(picture.media.node_id()));
            }
        }
        GroupChild::Group(group) => {
            if depth + 1 > MAX_GROUP_DEPTH {
                return Err(PresentationError::GroupNestingTooDeep(group.id));
            }
            for nested in &group.children {
                validate_child(nested, definitions, depth + 1)?;
            }
        }
        GroupChild::Shape(_) | GroupChild::TextBox(_) => {}
    }
    Ok(())
}
