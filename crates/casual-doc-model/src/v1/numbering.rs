//! The **one** answer to "which level does this `w:numPr` paint?".
//!
//! # Why this module exists
//!
//! Three readings of a `NumberingRef` used to disagree, each reimplementing the
//! rule at a different strictness:
//!
//! | Reader | Rule it applied |
//! | --- | --- |
//! | import (`casual-doc-import`) | the level must be a `w:lvl` child of the abstract the instance names |
//! | model validation | the level must be in `abstract_num.levels` of the abstract the instance names |
//! | layout (`casual-doc-layout`) | a `w:lvlOverride/w:lvl` redefinition, else the level of the abstract reached by following `w:numStyleLink` |
//!
//! The consequence was a class of real Word documents — anything authored with a
//! **List Style**, whose abstract carries `<w:numStyleLink/>` and no `w:lvl` at
//! all — importing with **no marker on any paragraph of the list**, while the
//! layout engine could already render exactly that shape and was tested on it
//! (`docs/142` LST-10). Fixing the importer alone would have traded a missing
//! marker for a *rejected document*, because the model's validator was the
//! stricter of the two (`docs/142` LST-34).
//!
//! So the rule lives **here**, in the lowest crate the other two both depend on,
//! and this crate is the authority:
//!
//! - [`Document::validate`](crate::v1::Document::validate) accepts a reference
//!   exactly when [`NumberingResolver::level`] resolves it;
//! - `casual-doc-layout` paints the level [`NumberingResolver::level`] returns;
//! - `casual-doc-import` admits a `w:numPr` exactly when
//!   [`NumberingResolver::level`] resolves it, and reports a finding otherwise.
//!
//! None of the three carries a rule of its own, so they cannot drift apart —
//! which is the failure mode `inline_text_len`, `append_node_plain_text` and
//! `inline_anchor_len_for_review` each shipped once.
//!
//! # The resolution order
//!
//! 1. A per-instance `w:lvlOverride/w:lvl` **full redefinition** for that level
//!    replaces everything below (its own `numFmt`/`lvlText`/`start`/`suff`/…).
//! 2. Otherwise the level of the **effective abstract**: the abstract the
//!    instance names, or — when that abstract defers with `w:numStyleLink` — the
//!    abstract reached by following the link through the named List-Style
//!    paragraph style to the instance it points at.
//!
//! A `w:startOverride` is *not* part of this: it adjusts the start value of an
//! already-resolved level, and its consumer is the counter engine.

use super::{
    AbstractNumbering, AbstractNumberingId, DefinitionMap, NumberingInstance, NumberingInstanceId,
    NumberingLevel, NumberingRef, Style, StyleId,
};

/// How many `w:numStyleLink` hops are followed before the chain is abandoned.
///
/// Word's own List-Style indirection is a single hop; the bound exists only so a
/// `numStyleLink`/`styleLink` **cycle** in a hostile or corrupt document cannot
/// loop forever. Eight is far beyond anything a producer writes and keeps the
/// walk O(1).
const MAX_NUM_STYLE_LINK_HOPS: usize = 8;

/// A borrowed view of the three tables numbering resolution reads, so the one
/// rule can be applied by a caller that has not (yet) assembled a
/// [`Definitions`](crate::v1::Definitions) — the importer resolves every
/// `w:numPr` while the document is still being built.
///
/// Construct one from a `Definitions` with
/// [`Definitions::numbering_resolver`](crate::v1::Definitions::numbering_resolver),
/// or from the three maps directly with [`NumberingResolver::new`].
#[derive(Clone, Copy, Debug)]
pub struct NumberingResolver<'a> {
    styles: &'a DefinitionMap<StyleId, Style>,
    instances: &'a DefinitionMap<NumberingInstanceId, NumberingInstance>,
    abstracts: &'a DefinitionMap<AbstractNumberingId, AbstractNumbering>,
}

impl<'a> NumberingResolver<'a> {
    /// A resolver over the three tables numbering resolution reads.
    ///
    /// `styles` is needed because the `w:numStyleLink` indirection routes
    /// *through* a paragraph style; a caller with no styles table can pass an
    /// empty map, and a `numStyleLink` will then simply fail to resolve (which
    /// its callers report — it is never silently defaulted).
    ///
    /// Complexity: O(1). It stores three references and copies nothing.
    #[must_use]
    pub fn new(
        styles: &'a DefinitionMap<StyleId, Style>,
        instances: &'a DefinitionMap<NumberingInstanceId, NumberingInstance>,
        abstracts: &'a DefinitionMap<AbstractNumberingId, AbstractNumbering>,
    ) -> Self {
        Self {
            styles,
            instances,
            abstracts,
        }
    }

    /// The instance a reference names, or `None` for a dangling `numId`.
    ///
    /// Complexity: O(log N) in the number of instances (one B-tree lookup).
    #[must_use]
    pub fn instance(&self, instance: NumberingInstanceId) -> Option<&'a NumberingInstance> {
        self.instances.get(&instance)
    }

    /// The abstract definition that actually defines `abstract_num`'s levels:
    /// `abstract_num` itself, or — when it defers with `w:numStyleLink` — the
    /// abstract reached by following that link.
    ///
    /// An abstract may defer its numbering to a numbering-defining paragraph
    /// style (`w:numStyleLink`), whose `w:pPr/w:numPr` points back at the
    /// instance — and so the abstract — that holds the real levels (the
    /// reusable-definition side carries the matching `w:styleLink`). A chain that
    /// dangles at any hop, or revisits the abstract it started from, stops and
    /// returns the last abstract reached; a caller that needs to know the link
    /// went nowhere sees it as "this abstract defines no such level" and reports.
    ///
    /// A style's *inherited* (`w:basedOn`) numbering is deliberately not walked:
    /// Word writes the `w:numPr` on the List Style itself, and inheriting it
    /// would make the resolution depend on a style cascade that
    /// [`Document::validate`](crate::v1::Document::validate) checks separately.
    ///
    /// Complexity: O(1) — at most `MAX_NUM_STYLE_LINK_HOPS` hops, each three
    /// B-tree lookups, so O(log S + log N + log A) per hop with a constant bound
    /// on the hop count. Independent of how many abstracts the document holds.
    #[must_use]
    pub fn effective_abstract(&self, abstract_num: &'a AbstractNumbering) -> &'a AbstractNumbering {
        let mut current = abstract_num;
        for _ in 0..MAX_NUM_STYLE_LINK_HOPS {
            let Some(style_id) = current.num_style_link else {
                break;
            };
            let Some(style) = self.styles.get(&style_id) else {
                break;
            };
            let Some(reference) = style.paragraph.as_ref().and_then(|p| p.numbering) else {
                break;
            };
            let Some(instance) = self.instances.get(&reference.instance) else {
                break;
            };
            let Some(next) = self.abstracts.get(&instance.abstract_ref) else {
                break;
            };
            if std::ptr::eq(next, current) {
                break;
            }
            current = next;
        }
        current
    }

    /// The level a paragraph carrying `reference` paints, or `None` when the
    /// reference resolves to no level at all (a dangling `numId`, a dangling
    /// `abstractNumId`, or an `ilvl` no reachable definition declares).
    ///
    /// **This predicate is the definition of a valid `NumberingRef`**: the model
    /// rejects a document whose paragraph carries a reference this returns `None`
    /// for, and the importer refuses (and reports) a `w:numPr` this returns
    /// `None` for. They agree because they call this, not because they each
    /// implement it.
    ///
    /// Complexity: O(L) in the number of levels the effective abstract declares
    /// (at most nine in OOXML) plus the O(1) link walk above; the per-instance
    /// override list is scanned once and is likewise bounded by nine. Not
    /// quadratic in abstracts or levels, and called once per `w:numPr`.
    #[must_use]
    pub fn level(&self, reference: NumberingRef) -> Option<&'a NumberingLevel> {
        let instance = self.instances.get(&reference.instance)?;
        let declared = self.abstracts.get(&instance.abstract_ref)?;
        self.level_of(instance, declared, reference.level)
    }

    /// [`NumberingResolver::level`] for a caller that already holds the instance
    /// and its declared abstract — the counter engine resolves a *different*
    /// level of the same instance for every `%n` placeholder in a `lvlText`, and
    /// re-looking-up the instance per placeholder would be a lookup-by-id inside
    /// a loop over ids.
    ///
    /// `declared` is the abstract the instance names; the `w:numStyleLink` walk
    /// happens here, so a caller never has to remember to do it.
    ///
    /// Complexity: as [`NumberingResolver::level`], minus the instance lookup.
    #[must_use]
    pub fn level_of(
        &self,
        instance: &'a NumberingInstance,
        declared: &'a AbstractNumbering,
        level: u8,
    ) -> Option<&'a NumberingLevel> {
        // 1. A per-instance `w:lvlOverride/w:lvl` full redefinition wins outright.
        if let Some(definition) = instance
            .overrides
            .iter()
            .find(|over| over.level == level)
            .and_then(|over| over.definition.as_ref())
        {
            return Some(definition);
        }
        // 2. Otherwise the level of the effective (link-followed) abstract.
        self.effective_abstract(declared)
            .levels
            .iter()
            .find(|candidate| candidate.level == level)
    }
}
