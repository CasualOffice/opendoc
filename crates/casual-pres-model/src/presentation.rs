// SPDX-License-Identifier: Apache-2.0

//! The presentation envelope, its three part kinds, and strict validation.

use std::collections::BTreeSet;

use casual_doc_model::v1::{Definitions, Fill, visit_definition_node_ids};
use casual_doc_model::{ModelError, NodeId};
use serde::{Deserialize, Serialize};

use crate::{
    LayoutKind, PlaceholderKind, PresentationError, ShapeTree, SlideId, SlideLayoutId,
    SlideMasterId, SlideSize, TextStyles,
};
// Own line (anti-conflict): the presentation-wide bottom tier of the text cascade.
use crate::text_body::ListStyle;
// Own line (anti-conflict): the theme indirection, which has no document analogue.
use crate::theme::{ColorMap, ColorMapping, ThemeFontReference, ThemePalette, validate_mapping};

/// The schema version stamped on a presentation.
///
/// Version 1 from the start, not 0: the document model's v0 exists only because a
/// pre-typed schema shipped before v1 did, and there is no such history to carry
/// here.
pub const SCHEMA_VERSION: u32 = 1;

/// A slide (`p:sld`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slide {
    /// Stable identity.
    pub id: SlideId,
    /// The layout this slide inherits through (`p:sld` -> `slideLayout`
    /// relationship). Required: a slide with no layout has nothing to resolve a
    /// placeholder against, and PowerPoint always writes one.
    pub layout: SlideLayoutId,
    /// The shapes on the slide (`p:cSld/p:spTree`).
    pub shapes: ShapeTree,
    /// The author-visible slide name (`p:cSld@name`), when the file carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the slide is skipped during a show (`p:sld@show="0"`).
    ///
    /// Retained rather than dropped: a hidden slide is still in the deck, still
    /// edited, and still exported. Named `hidden` rather than mirroring the
    /// attribute's `show` polarity so the default is `false` and an absent
    /// attribute needs no special case.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hidden: bool,
    /// The slide background (`p:cSld/p:bg`), overriding the layout's. `None` means
    /// "inherit", which is the overwhelmingly common case and must stay
    /// distinguishable from an explicit white.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Fill>,
}

/// A slide layout (`p:sldLayout`): the middle tier of the inheritance cascade.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideLayout {
    /// Stable identity.
    pub id: SlideLayoutId,
    /// The master this layout inherits through.
    pub master: SlideMasterId,
    /// What the layout is for (`p:sldLayout@type`).
    #[serde(default)]
    pub kind: LayoutKind,
    /// The layout's own shapes and placeholder slots (`p:cSld/p:spTree`).
    pub shapes: ShapeTree,
    /// The layout's name as the gallery shows it (`p:cSld@name`), e.g. "Title and
    /// Content".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The layout background, overriding the master's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Fill>,
}

/// A slide master (`p:sldMaster`): the root of the inheritance cascade.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideMaster {
    /// Stable identity.
    pub id: SlideMasterId,
    /// The master's own shapes and placeholder slots (`p:cSld/p:spTree`).
    pub shapes: ShapeTree,
    /// The master's name (`p:cSld@name`), when the file carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The master background, the last fallback in the cascade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Fill>,
    /// The three text-style tiers (`p:txStyles`) every shape on a slide using this
    /// master inherits its text properties from, after its own shape and its
    /// layout.
    ///
    /// Carried on the master and NOT resolved into each shape, for the same reason
    /// the placeholder geometry is not: a round trip must not turn inheritance into
    /// authorship. [`TextStyles::tier`] picks which of the three applies.
    #[serde(default, skip_serializing_if = "TextStyles::is_empty")]
    pub text_styles: TextStyles,
}

/// A normalized presentation.
///
/// # Why a separate envelope rather than a `Document` profile
///
/// `casual_doc_model::v1::Document` is `body: Vec<BlockNode>` plus `Definitions`: a
/// linear flow paginated onto a page size that varies per section. A presentation is
/// an ordered set of fixed-size surfaces, each positioning shapes absolutely and
/// inheriting through two further tiers. Nothing about the flow is reused, so
/// expressing a deck as a `Document` would mean a body that is never flowed, a
/// section that never breaks, and a pagination pass that must be skipped — three
/// invariants weakened in the type every DOCX caller depends on, for no gain.
/// ADR-055 settles this: a second document class is **additive**, and
/// `v1::Document` is not modified.
///
/// What *is* shared is everything below the document class: node identity, the
/// `Definitions` tables, and the whole DrawingML vocabulary. That sharing is by
/// direct reuse of those types, not by a common supertype.
///
/// # What this does not yet model
///
/// Stated rather than left ambiguous (`SKILL` §8): notes slides and handout masters,
/// transitions (`p:transition`), animation (`p:timing`), and slide sections
/// (`p14:sectionLst`). None of them are forward-incompatible with this envelope;
/// each is additive.
///
/// The `p:txStyles` tiers and `p:defaultTextStyle` **are** modelled now
/// ([`SlideMaster::text_styles`], [`Presentation::default_text_style`]), so every
/// tier of the text cascade is carried. What is not built is the RESOLVER that
/// folds them — a run with no stated size still has no resolved size, and
/// `casual-pres-layout` still draws no glyphs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Presentation {
    schema_version: u32,
    presentation_id: NodeId,
    slide_size: SlideSize,
    masters: Vec<SlideMaster>,
    layouts: Vec<SlideLayout>,
    slides: Vec<Slide>,
    definitions: Definitions,
    #[serde(default, skip_serializing_if = "ListStyle::is_empty")]
    default_text_style: ListStyle,
    #[serde(default, skip_serializing_if = "ColorMapping::is_empty")]
    color_mapping: ColorMapping,
}

impl Presentation {
    /// Builds and validates a presentation from constructed parts.
    ///
    /// `slides` is in **presentation order** — the order `p:sldIdLst` gives, which
    /// is the order the deck is shown in. Reordering a deck is a move within this
    /// vector and nothing else; there is no separate ordering key to keep in step.
    ///
    /// # Complexity
    ///
    /// O(presentation), because it validates. Call it at open, at save and at
    /// export — never per keystroke (`docs/107` §4).
    pub fn new(
        presentation_id: NodeId,
        slide_size: SlideSize,
        masters: Vec<SlideMaster>,
        layouts: Vec<SlideLayout>,
        slides: Vec<Slide>,
        definitions: Definitions,
    ) -> Result<Self, PresentationError> {
        let presentation = Self {
            schema_version: SCHEMA_VERSION,
            presentation_id,
            slide_size,
            masters,
            layouts,
            slides,
            definitions,
            // The bottom tier is set through `with_default_text_style` rather than
            // taken here. Adding a seventh positional argument would be a breaking
            // change to every caller for a part most packages do not carry, and
            // `docs/156` §5a is about exactly that kind of churn.
            default_text_style: ListStyle::default(),
            // Same reasoning, and one more: the colour map is keyed by part id, so
            // it can only be built once the ids above exist.
            color_mapping: ColorMapping::default(),
        };
        presentation.validate()?;
        Ok(presentation)
    }

    /// Attaches the presentation-wide `p:defaultTextStyle`: the LAST tier a run's
    /// properties resolve through before the engine's own defaults.
    ///
    /// Re-validates, because a list style deeper than nine levels is refused
    /// wherever it appears and this one arrives after construction.
    pub fn with_default_text_style(
        mut self,
        default_text_style: ListStyle,
    ) -> Result<Self, PresentationError> {
        self.default_text_style = default_text_style;
        self.validate()?;
        Ok(self)
    }

    /// Attaches the colour maps the parts state (`p:clrMap` on each master,
    /// `p:clrMapOvr/a:overrideClrMapping` on a layout or slide that overrides it).
    ///
    /// Re-validates, because a key naming a part this deck does not hold would make
    /// [`ColorMapping::in_force`] fall through and resolve a colour role to the
    /// wrong theme slot.
    ///
    /// # Errors
    ///
    /// [`PresentationError::DanglingColorMapRef`] when a key names no part of this
    /// presentation.
    pub fn with_color_mapping(
        mut self,
        color_mapping: ColorMapping,
    ) -> Result<Self, PresentationError> {
        self.color_mapping = color_mapping;
        self.validate()?;
        Ok(self)
    }

    /// The colour maps the deck's parts state.
    #[must_use]
    pub const fn color_mapping(&self) -> &ColorMapping {
        &self.color_mapping
    }

    /// The colour map in force for one slide, following the override chain.
    ///
    /// # Complexity
    ///
    /// O(layouts + masters) — the two tier lookups are scans over the layout and
    /// master lists, not over the deck. Hoist it out of a loop over slides.
    #[must_use]
    pub fn color_map_of(&self, slide: &Slide) -> ColorMap {
        let layout = self.layout_of(slide);
        self.color_mapping.in_force(
            slide.id,
            layout.map(|layout| layout.id),
            layout.map(|layout| layout.master),
        )
    }

    /// The resolved theme palette in force for one slide: the deck's `a:clrScheme`
    /// read through that slide's effective colour map.
    ///
    /// `None` when the package carried no theme colour scheme, which is the one
    /// honest answer — a substituted Office palette would paint a branded deck in
    /// the wrong brand and look deliberate.
    ///
    /// # Complexity
    ///
    /// O(layouts + masters), as [`Presentation::color_map_of`].
    #[must_use]
    pub fn theme_palette_of(&self, slide: &Slide) -> Option<ThemePalette> {
        let scheme = self.definitions.color_scheme.as_ref()?;
        Some(ThemePalette::new(scheme, self.color_map_of(slide)))
    }

    /// The concrete family a typeface names, following a `+mj-lt`-style theme
    /// reference through the deck's `a:fontScheme`.
    ///
    /// A concrete family is returned unchanged, so this is the one call a consumer
    /// needs; `None` means the reference does not resolve (no font scheme, or an
    /// entry whose `@typeface` is the empty "fall back to latin" marker) and the
    /// caller must choose, rather than being handed a fabricated family.
    ///
    /// The reference is NOT folded into the run at import: `+mj-lt` is what the
    /// file says, and rewriting it as `Calibri Light` would turn a theme reference
    /// into authorship, so a later theme change would stop following.
    ///
    /// # Complexity
    ///
    /// O(1).
    #[must_use]
    pub fn resolve_typeface<'a>(&'a self, typeface: &'a crate::Typeface) -> Option<&'a str> {
        match ThemeFontReference::parse(&typeface.name) {
            Some(reference) => reference.resolve(self.definitions.font_scheme.as_ref()?),
            None => Some(typeface.name.as_str()),
        }
    }

    /// The presentation-wide `p:defaultTextStyle`, empty when the package carries
    /// none.
    #[must_use]
    pub const fn default_text_style(&self) -> &ListStyle {
        &self.default_text_style
    }

    /// The schema version (always [`SCHEMA_VERSION`] for a valid presentation).
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// The presentation's own id.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.presentation_id
    }

    /// The surface every slide is laid out on.
    #[must_use]
    pub const fn slide_size(&self) -> SlideSize {
        self.slide_size
    }

    /// The slides, in presentation order.
    #[must_use]
    pub fn slides(&self) -> &[Slide] {
        &self.slides
    }

    /// The slides for in-place editing. Callers preserve the invariants, which are
    /// re-checked on save and export — the same contract as
    /// `v1::Document::body_mut`.
    #[must_use]
    pub fn slides_mut(&mut self) -> &mut Vec<Slide> {
        &mut self.slides
    }

    /// The layouts.
    #[must_use]
    pub fn layouts(&self) -> &[SlideLayout] {
        &self.layouts
    }

    /// The masters.
    #[must_use]
    pub fn masters(&self) -> &[SlideMaster] {
        &self.masters
    }

    /// The shared definition tables (theme, media, fonts).
    #[must_use]
    pub const fn definitions(&self) -> &Definitions {
        &self.definitions
    }

    /// Mutable access to the definition tables, for registering infrastructure an
    /// edit references (a newly embedded image's media entry, say).
    #[must_use]
    pub fn definitions_mut(&mut self) -> &mut Definitions {
        &mut self.definitions
    }

    /// The layout a slide inherits through.
    ///
    /// # Complexity
    ///
    /// O(layouts) — a linear scan, bounded by the deck's layout count (a dozen in a
    /// normal deck, since layouts are shared across slides) and NOT by the slide
    /// count. Never call it inside a loop over slides without hoisting; see
    /// [`Presentation::resolve_layout_indices`], which does one pass instead of one
    /// scan per slide.
    #[must_use]
    pub fn layout_of(&self, slide: &Slide) -> Option<&SlideLayout> {
        self.layouts.iter().find(|layout| layout.id == slide.layout)
    }

    /// The master a layout inherits through.
    ///
    /// # Complexity
    ///
    /// O(masters), which is one or two in almost every real deck.
    #[must_use]
    pub fn master_of(&self, layout: &SlideLayout) -> Option<&SlideMaster> {
        self.masters
            .iter()
            .find(|master| master.id == layout.master)
    }

    /// Every slide's layout index, resolved in **one** pass over the layouts.
    ///
    /// # Why this exists
    ///
    /// [`Presentation::layout_of`] is a linear scan that looks like an accessor at
    /// the call site, and calling it once per slide is O(slides x layouts) — the
    /// exact shape that shipped an O(n^2) on the document outline path
    /// (`SKILL` §8). Anything that walks the whole deck resolves layouts through
    /// this instead.
    ///
    /// # Complexity
    ///
    /// O(slides + layouts).
    #[must_use]
    pub fn resolve_layout_indices(&self) -> Vec<Option<usize>> {
        let index: std::collections::BTreeMap<SlideLayoutId, usize> = self
            .layouts
            .iter()
            .enumerate()
            .map(|(position, layout)| (layout.id, position))
            .collect();
        self.slides
            .iter()
            .map(|slide| index.get(&slide.layout).copied())
            .collect()
    }

    /// The shape filling `slot` for `slide`, honouring the cascade: the slide's own
    /// shape if it has one, else the layout's, else the master's.
    ///
    /// This is the lookup that makes a deck render at all — a title shape on a real
    /// slide usually carries only its text, and takes its position, size and font
    /// from the tier above.
    ///
    /// # Complexity
    ///
    /// O(layouts + masters + shapes-per-tree). The scans are over the layout and
    /// master lists, not the deck, so this is independent of slide count.
    #[must_use]
    pub fn resolve_slot<'a>(
        &'a self,
        slide: &'a Slide,
        kind: PlaceholderKind,
        index: u32,
    ) -> Option<&'a crate::SlideNode> {
        if let Some(node) = slide.shapes.slot(kind, index) {
            return Some(node);
        }
        let layout = self.layout_of(slide)?;
        if let Some(node) = layout.shapes.slot(kind, index) {
            return Some(node);
        }
        self.master_of(layout)?.shapes.slot(kind, index)
    }

    /// The text inheritance chain in effect for one shape on one slide.
    ///
    /// Built once per shape and then queried per outline level, because the two
    /// placeholder-slot lookups scan a layout's and a master's children: doing them
    /// per paragraph would be `O(paragraphs x shapes)`, which is the shape of the
    /// `O(n^2)` that shipped on the outline path (`SKILL` §8).
    ///
    /// A shape in no slot still inherits, but through a different pair of tiers: it
    /// takes the master's `p:otherStyle` AND `p:defaultTextStyle`, with the two slot
    /// tiers empty. A PLACEHOLDER takes its slot chain and its matching master tier
    /// and **not** `p:defaultTextStyle` — ECMA-376 §19.2.1.8 scopes that part to
    /// text "not in a placeholder", so applying it to a placeholder would let a
    /// deck-wide default leak past the master tier that is supposed to govern it.
    /// The one exception is a master with no `p:txStyles` at all: there is then no
    /// tier for the placeholder to inherit from, and the deck default is all there
    /// is.
    ///
    /// # Complexity
    ///
    /// O(layouts + masters + shapes-per-tree), independent of the slide count.
    #[must_use]
    pub fn text_cascade<'a>(
        &'a self,
        slide: &'a Slide,
        node: &'a crate::SlideNode,
    ) -> crate::TextCascade<'a> {
        let layout = self.layout_of(slide);
        let master = layout.and_then(|layout| self.master_of(layout));
        let list_style_of = |node: Option<&'a crate::SlideNode>| {
            node.and_then(|node| node.text.as_ref())
                .map(|text| &text.list_style)
        };
        // The slot tiers apply only to a shape that IS in a slot; a shape with no
        // `p:ph` matches nothing on the layout or the master, and asking for slot
        // `None` would silently match the first unplaced shape there.
        let slot = node.placeholder.map(|placeholder| placeholder.slot());
        crate::TextCascade {
            shape: list_style_of(Some(node)),
            layout: slot.and_then(|(kind, index)| {
                list_style_of(layout.and_then(|layout| layout.shapes.slot(kind, index)))
            }),
            master_slot: slot.and_then(|(kind, index)| {
                list_style_of(master.and_then(|master| master.shapes.slot(kind, index)))
            }),
            master_tier: master.map(|master| {
                master
                    .text_styles
                    .tier(node.placeholder.map(|slot| slot.kind))
            }),
            // Scoped, not unconditional — see the note above.
            deck: (node.placeholder.is_none()
                || master.is_none_or(|master| master.text_styles.is_empty()))
            .then_some(&self.default_text_style),
        }
    }

    /// Visits every node id in the presentation, in a deterministic order.
    ///
    /// Shares the document model's definition and drawing traversals rather than
    /// repeating them, so the two document classes cannot disagree about what a
    /// node id is.
    ///
    /// # Complexity
    ///
    /// O(presentation). Not for a keystroke.
    pub fn visit_node_ids(
        &self,
        visit: &mut dyn FnMut(NodeId) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        visit(self.presentation_id)?;
        visit_definition_node_ids(&self.definitions, visit)?;
        for master in &self.masters {
            visit(master.id.node_id())?;
            master.shapes.visit_node_ids(visit)?;
        }
        for layout in &self.layouts {
            visit(layout.id.node_id())?;
            layout.shapes.visit_node_ids(visit)?;
        }
        for slide in &self.slides {
            visit(slide.id.node_id())?;
            slide.shapes.visit_node_ids(visit)?;
        }
        Ok(())
    }

    /// Every node id the presentation carries.
    ///
    /// # Complexity
    ///
    /// O(presentation): the same walk [`Presentation::validate`] makes.
    #[must_use]
    pub fn node_ids(&self) -> BTreeSet<NodeId> {
        let mut ids = BTreeSet::new();
        // The closure never returns `Err`, so the walk is total and the `unwrap_or`
        // is unreachable rather than a swallowed failure.
        let () = self
            .visit_node_ids(&mut |id| {
                ids.insert(id);
                Ok(())
            })
            .unwrap_or(());
        ids
    }

    /// Checks every presentation invariant.
    ///
    /// # Complexity
    ///
    /// O(presentation): one id walk plus one pass per part. Not for a keystroke.
    pub fn validate(&self) -> Result<(), PresentationError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(PresentationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.slides.is_empty() {
            return Err(PresentationError::EmptyPresentation);
        }
        self.slide_size.validate()?;
        self.validate_unique_ids()?;
        self.validate_references()?;
        for master in &self.masters {
            master.shapes.validate(&self.definitions)?;
        }
        for layout in &self.layouts {
            layout.shapes.validate(&self.definitions)?;
        }
        for master in &self.masters {
            // All three `p:txStyles` tiers, not just the one a given shape reads:
            // an unvalidated tier is a nine-level invariant that holds only for the
            // shapes that happen to use the tier somebody checked.
            for tier in [
                &master.text_styles.title,
                &master.text_styles.body,
                &master.text_styles.other,
            ] {
                tier.validate()?;
            }
        }
        self.default_text_style.validate()?;
        for slide in &self.slides {
            slide.shapes.validate(&self.definitions)?;
        }
        validate_mapping(
            &self.color_mapping,
            &self.masters,
            &self.layouts,
            &self.slides,
        )?;
        Ok(())
    }

    /// Refuses a node id that appears twice anywhere in the presentation.
    fn validate_unique_ids(&self) -> Result<(), PresentationError> {
        let mut seen = BTreeSet::new();
        self.visit_node_ids(&mut |id| {
            if seen.insert(id) {
                Ok(())
            } else {
                Err(ModelError::DuplicateNodeId(id))
            }
        })
        .map_err(|error| match error {
            ModelError::DuplicateNodeId(id) => PresentationError::DuplicateNodeId(id),
            other => PresentationError::Model(other),
        })
    }

    /// Refuses a slide whose layout, or a layout whose master, does not resolve.
    ///
    /// # Complexity
    ///
    /// O(slides + layouts + masters), via two id sets rather than a scan per
    /// reference.
    fn validate_references(&self) -> Result<(), PresentationError> {
        let layouts: BTreeSet<SlideLayoutId> =
            self.layouts.iter().map(|layout| layout.id).collect();
        let masters: BTreeSet<SlideMasterId> =
            self.masters.iter().map(|master| master.id).collect();
        for layout in &self.layouts {
            if !masters.contains(&layout.master) {
                return Err(PresentationError::DanglingMasterRef(layout.id));
            }
        }
        for slide in &self.slides {
            if !layouts.contains(&slide.layout) {
                return Err(PresentationError::DanglingLayoutRef(slide.id));
            }
        }
        Ok(())
    }
}
