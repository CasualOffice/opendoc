//! A legacy VML drawing carried verbatim beside its DrawingML projection
//! (`docs/08` ADR-072, `docs/109` HF-395).
//!
//! # The pattern
//!
//! The importer NORMALISES a `w:pict` into the typed DrawingML model (groups,
//! shapes, pictures, text boxes), which is what the renderer draws and the
//! editor edits. The source is kept beside the projection with a FINGERPRINT
//! of the projection as it was imported — copy-on-write with a dirty check. A
//! save that finds the projection unchanged writes the source back, so a
//! shape nobody touched leaves as the VML it arrived as; a save that finds it
//! changed writes the projection as DrawingML and says so in the export
//! report, because the VML-only detail (formulas, handles, `o:` extensions)
//! is then gone.
//!
//! The text-box CONTENT is deliberately outside the fingerprint and outside
//! the verbatim source: the source is stored split at each `w:txbxContent`, and
//! the writer fills each gap with the text box's current blocks. That is not
//! a convenience. Text-box content holds style ids, numbering ids, bookmark
//! ids and relationship ids, all of which the writer regenerates; spliced in
//! verbatim they would dangle. And it means typing in a VML text box keeps the
//! box VML.
//!
//! # Why a side table
//!
//! The projection's nodes are ordinary model nodes (`TextBox`,
//! `WordprocessingGroup`, `AnchoredDrawing`, …); a field on each would break
//! every literal of four widely built structs for a datum only VML carries,
//! the reason `Definitions::shape_styles` and `Definitions::object_names` are
//! side tables too. The entry is keyed by the FIRST member's node id.

use serde::{Deserialize, Serialize};

use super::{BlockNode, GroupChild, InlineNode};
use crate::NodeId;

/// One `w:pict`, verbatim, and the projection it became.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyVml {
    /// The `w:pict` element, self-contained — every namespace prefix it uses is
    /// declared on it — and split at each outermost `w:txbxContent`'s content:
    /// `source.len() == text_boxes.len() + 1`, and the element is
    /// `source[0] + content(text_boxes[0]) + source[1] + …`.
    pub source: Vec<String>,
    /// The inline nodes the pict became, in document order. They are adjacent
    /// siblings in one inline sequence when the projection is unchanged.
    pub members: Vec<NodeId>,
    /// The text boxes (a `TextBox` member, or a `GroupTextBox` inside a member
    /// group) whose blocks fill the gaps in `source`, in order.
    pub text_boxes: Vec<NodeId>,
    /// [`legacy_vml_fingerprint`] of the members at import.
    pub fingerprint: String,
}

/// The fingerprint of a pict's projection: FNV-1a 64 over the members' JSON
/// with every text box's blocks removed, as sixteen hex digits.
///
/// Hex rather than a number because the model crosses into JavaScript as JSON,
/// where a 64-bit integer loses its low bits. FNV rather than `DefaultHasher`
/// because the value is persisted in snapshots and must not change between
/// toolchains. A collision would write a stale VML source over an edit; at
/// 2^-64 per edited pict that is below every other failure rate in the save
/// path.
///
/// O(size of the members), text excluded.
#[must_use]
pub fn legacy_vml_fingerprint(members: &[&InlineNode]) -> String {
    let stripped: Vec<InlineNode> = members
        .iter()
        .map(|node| {
            let mut node = (*node).clone();
            strip_text_box_blocks(&mut node);
            node
        })
        .collect();
    // Serialization of a model node cannot fail (no maps with non-string
    // keys); an empty buffer would still be a stable, if weaker, fingerprint.
    let bytes = serde_json::to_vec(&stripped).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn strip_text_box_blocks(node: &mut InlineNode) {
    // container-set: a VML member is a TextBox or a Group only; the other four containers never hold a pict member, so this walk deliberately names just those two.
    match node {
        InlineNode::TextBox(text_box) => text_box.blocks = Vec::<BlockNode>::new(),
        InlineNode::Group(group) => strip_children(&mut group.children),
        _ => {}
    }
}

fn strip_children(children: &mut [GroupChild]) {
    for child in children {
        match child {
            GroupChild::TextBox(text_box) => text_box.blocks = Vec::new(),
            GroupChild::Group(group) => strip_children(&mut group.children),
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
}

/// The blocks of text box `id` among `members` — a top-level `TextBox`, or a
/// `GroupTextBox` at any depth inside a member group. `None` when no member
/// holds it.
#[must_use]
pub fn legacy_vml_text_box_blocks<'a>(
    members: &[&'a InlineNode],
    id: NodeId,
) -> Option<&'a [BlockNode]> {
    fn in_children(children: &[GroupChild], id: NodeId) -> Option<&[BlockNode]> {
        children.iter().find_map(|child| match child {
            GroupChild::TextBox(text_box) if text_box.id == id => Some(text_box.blocks.as_slice()),
            GroupChild::Group(group) => in_children(&group.children, id),
            _ => None,
        })
    }
    // container-set: VML members are TextBox or Group only (the other four containers are never pict members), so only those two are searched.
    members.iter().find_map(|node| match node {
        InlineNode::TextBox(text_box) if text_box.id == id => Some(text_box.blocks.as_slice()),
        InlineNode::Group(group) => in_children(&group.children, id),
        _ => None,
    })
}
