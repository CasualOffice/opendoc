//! Writing a legacy VML drawing back as the VML it arrived as (`docs/08`
//! ADR-072, `docs/109` HF-395).
//!
//! The importer normalises a `w:pict` into DrawingML model nodes and keeps the
//! source beside them in `Definitions::legacy_vml`, with a fingerprint of the
//! projection. This module is the writer's half of that copy-on-write
//! contract: it answers "may this run of inline nodes be written as its
//! source?" and remembers, across every part of one package, which sources
//! were written and which projections were written INSTEAD — the latter are
//! named in the export report, because the VML-only detail of an edited shape
//! is then gone.
//!
//! The decision is per run of members and is all-or-nothing: the source is
//! written only when every member is present, adjacent, in order, with the
//! fingerprint it was imported with, and every image it draws is in the
//! package. Anything else writes the projection, which is the edited state the
//! user sees.
//!
//! Complexity: building the member index is O(entries × members); each check
//! is O(size of the members), and only runs at the first member of an entry.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, DefinitionMap, Definitions, GroupChild, InlineNode, LegacyVml, MediaId,
    MediaReference, legacy_vml_fingerprint, legacy_vml_text_box_blocks,
};

use crate::report::{Disposition, Reporter};

/// The feature id the export report names a regenerated VML drawing by.
pub(crate) const REGENERATED_FEATURE: &str = "docx.export.vml.regenerated";

/// The feature id for an even-odd geometry written without its fill rule
/// (`docs/182` G-2).
pub(crate) const FILL_RULE_FEATURE: &str = "docx.export.custom_geometry.even_odd_fill";

/// One package's legacy VML bookkeeping, shared by every part's writer.
///
/// Interior mutability for the reason `DocPrIds` gives: the parts are written
/// one after another and each holds this by shared reference.
#[derive(Debug, Default)]
pub(crate) struct LegacyVmlWrites {
    /// Every member node id, to the entry (its lead) it belongs to.
    lead_of: BTreeMap<NodeId, NodeId>,
    /// The part now being written, for the report's location.
    part: RefCell<String>,
    /// Leads whose source was written.
    written: RefCell<BTreeSet<NodeId>>,
    /// Leads one of whose members was written as its projection, with the part.
    regenerated: RefCell<BTreeMap<NodeId, String>>,
    /// Even-odd custom geometries written without their fill rule, per part
    /// (`docs/182` G-2). Not a VML concern: it rides on this struct because this
    /// is the one per-package bookkeeping every part's writer already holds,
    /// with the part name it is writing — threading a second handle through
    /// every `Ctx` for one counter would be the larger change.
    fill_rule_losses: RefCell<BTreeMap<String, u32>>,
}

impl LegacyVmlWrites {
    /// The bookkeeping for `definitions`' legacy VML table.
    pub(crate) fn new(definitions: &Definitions) -> Self {
        let lead_of = definitions
            .legacy_vml
            .iter()
            .flat_map(|(lead, entry)| entry.members.iter().map(move |member| (*member, *lead)))
            .collect();
        Self {
            lead_of,
            ..Self::default()
        }
    }

    /// Names the part the next nodes are written into.
    pub(crate) fn enter_part(&self, part: &str) {
        part.clone_into(&mut self.part.borrow_mut());
    }

    /// The source entry `inlines` starts with, when the run may be written as
    /// that source: `Some((entry, text-box blocks))`.
    pub(crate) fn writable<'a>(
        definitions: &'a Definitions,
        media: &DefinitionMap<MediaId, MediaReference>,
        inlines: &'a [InlineNode],
    ) -> Option<(&'a LegacyVml, Vec<&'a [BlockNode]>)> {
        let first = inlines.first()?;
        let entry = definitions.legacy_vml.get(&first.id())?;
        let run = inlines.get(..entry.members.len())?;
        if !run
            .iter()
            .map(InlineNode::id)
            .eq(entry.members.iter().copied())
        {
            return None;
        }
        let members: Vec<&InlineNode> = run.iter().collect();
        if legacy_vml_fingerprint(&members) != entry.fingerprint {
            return None;
        }
        let mut drawn = BTreeSet::new();
        for member in &members {
            member_media(member, &mut drawn);
        }
        if !drawn.iter().all(|id| media.get(id).is_some()) {
            return None;
        }
        let blocks: Vec<&[BlockNode]> = entry
            .text_boxes
            .iter()
            .map(|id| legacy_vml_text_box_blocks(&members, *id))
            .collect::<Option<_>>()?;
        (entry.source.len() == blocks.len() + 1).then_some((entry, blocks))
    }

    /// Records that `lead`'s source was written.
    pub(crate) fn mark_written(&self, lead: NodeId) {
        self.written.borrow_mut().insert(lead);
    }

    /// Records that node `id` was written as its projection. When it is a
    /// member of a retained pict, that pict was regenerated.
    pub(crate) fn note_projection(&self, id: NodeId) {
        if let Some(lead) = self.lead_of.get(&id) {
            let part = self.part.borrow().clone();
            self.regenerated.borrow_mut().entry(*lead).or_insert(part);
        }
    }

    /// Records that an even-odd geometry was written in the current part
    /// without its fill rule: no re-winding paints it the same under non-zero.
    pub(crate) fn note_fill_rule_loss(&self) {
        let part = self.part.borrow().clone();
        *self.fill_rule_losses.borrow_mut().entry(part).or_insert(0) += 1;
    }

    /// Names every regenerated pict in the export report: shown, and saved as
    /// DrawingML, but no longer the VML it was — `degraded`, `not_retained`.
    /// Also names every even-odd geometry whose fill rule the package does not
    /// carry: the shape is written, its hole may fill — `degraded`,
    /// `not_retained`.
    pub(crate) fn report(&self, reporter: &mut Reporter) {
        for (part, count) in self.fill_rule_losses.borrow().iter() {
            for _ in 0..*count {
                reporter.record_construct(
                    FILL_RULE_FEATURE,
                    part,
                    "custGeom",
                    None,
                    Disposition::DegradedNotRetained,
                );
            }
        }
        let written = self.written.borrow();
        for (lead, part) in self.regenerated.borrow().iter() {
            if !written.contains(lead) {
                reporter.record_construct(
                    REGENERATED_FEATURE,
                    part,
                    "pict",
                    None,
                    Disposition::DegradedNotRetained,
                );
            }
        }
    }
}

/// The media a projected node draws (a picture, or a picture inside a group).
fn member_media(node: &InlineNode, out: &mut BTreeSet<MediaId>) {
    fn children(list: &[GroupChild], out: &mut BTreeSet<MediaId>) {
        for child in list {
            match child {
                GroupChild::Picture(picture) => {
                    out.insert(picture.media);
                }
                GroupChild::Group(group) => children(&group.children, out),
                GroupChild::TextBox(_) | GroupChild::Shape(_) => {}
            }
        }
    }
    match node {
        InlineNode::Drawing(drawing) => {
            out.insert(drawing.media);
        }
        InlineNode::AnchoredDrawing(drawing) => {
            out.insert(drawing.media);
        }
        InlineNode::Group(group) => children(&group.children, out),
        _ => {}
    }
}
