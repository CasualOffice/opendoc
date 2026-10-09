//! Moving an in-line object to a new place in the text — the drag Word and
//! Google Docs give a picture that sits in the line of text (`docs/109`
//! UX-OB-02).
//!
//! # The competitive standard
//!
//! Word and Docs both let a reader pick up an in-line picture and put it
//! somewhere else in the text. A drop caret follows the pointer; on release the
//! picture is cut from where it was and inserted at the caret, as ONE undo step.
//! Ctrl-drag (Option-drag on a Mac) leaves the original where it was and drops
//! a copy. Esc cancels, and dropping it back where it came from does nothing.
//! Word's keyboard twin is F2 — "Move to where?" in the status bar, move the
//! insertion point, Enter — and Shift+F2 copies the same way. Before this, the
//! same drag here only said why it would not move (`docs/109` HF-259).
//!
//! # Prior art, named before the code
//!
//! This is cut-and-paste of one node, expressed as a MOVE inside one
//! transaction: the `RemoveInlineObject` / `InsertInlineObject` pair that already
//! exists as each other's inverse. No new operation (the closed set, ADR-030 I2),
//! no second copy routine (`clone_inline_with_fresh_ids` is the clipboard's own
//! total copy), and no second hit-test: the host drops at whatever its click
//! hit-test resolved for the pointer.
//!
//! # Why the node is moved by identity rather than rebuilt
//!
//! The removal hands back the node itself and the insertion puts THAT node back,
//! under the same id. So every property the model holds for it travels without
//! being enumerated — the image, its size, crop, border, rotation and flips, alt
//! text, opacity and link — and so does everything a side table keys by that
//! id: its name and title (`Definitions::object_names`), its theme style
//! (`shape_styles`) and a chart's projection (`Definitions::charts`). Rebuilding
//! a node field by field is how a move comes to drop the one property nobody
//! listed. A chart's projection needs one more thing, and it is in the atomic
//! choke point rather than here: `cascade_chart_projections` must not delete
//! the projection of an object the same transaction puts back.
//!
//! A COPY cannot be the same node — two nodes cannot share an id — so it is the
//! clipboard's total copy with fresh ids. What a fresh id cannot carry is said
//! rather than dropped: a name starts empty, which `ObjectName`'s own
//! documentation records as the accepted cost of the side table; a theme style or
//! a chart projection would change what the copy LOOKS like, so a copy that would
//! lose one is refused with a reason, and so is a kind whose package part a copy
//! would have to share.
//!
//! # Complexity
//!
//! **O(document)** per call: one surface walk to find the object
//! (`inline_object_position`), two `surface_of` walks to compare stories, and the
//! walks the two operations make themselves. It runs once per gesture — on
//! release, never per pointer sample — like every other object command in
//! `objects.rs`. The per-pointer-move work is the host's: one `hitTest` and one
//! `caretRect`, which is what a drag-selection already costs.

use casual_doc_edit::clone::{CloneReport, clone_inline_with_fresh_ids, node_ids_of_inline};
// Its own line, not folded into a sorted block: a shared `use` list is where
// parallel lanes collide (rustfmt is set to Preserve).
use casual_doc_edit::refused;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::InlineNode;
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, node_id_msg, object_anchor_any_surface, to_js};

/// A floating object is placed by dragging it anywhere on the page; it has no
/// place in the text to move to. Also the `canMoveInText` reason for every
/// anchored carrier (a floating picture, a group, a member of a group).
pub(crate) const FLOATS_FREELY: &str = refused!(
    "object.floats-not-in-text",
    "This object floats over the page, so it is placed by dragging it anywhere rather \
     than into the text. Set its wrapping to In line with text to move it with the words."
);

/// An in-line object that is not directly in the text: it is the result of a
/// field, the content of a link, inside a content control or a tracked change.
/// Also the `canMoveInText` reason for such an object.
pub(crate) const NOT_DIRECTLY_IN_TEXT: &str = refused!(
    "object.move-in-wrapper",
    "This object is part of a link, a field, a content control or a tracked change, \
     so it cannot be dragged out of it on its own."
);

/// The drop point is not a place in text at all: a table, a row, a cell, or an
/// id nothing in the document carries.
const TARGET_NOT_TEXT: &str = refused!(
    "object.move-target-not-text",
    "That is not a place in the text an object can be moved to. Drop it between words \
     of a paragraph or a table cell."
);

/// A text box dropped into its own text.
const TARGET_INSIDE_ITSELF: &str = refused!(
    "object.move-into-itself",
    "A text box cannot be moved into its own text. Drop it outside the box."
);

/// The drop point is in another story — the body versus a header, a footer or a
/// note. Word keeps one story active at a time, and so does this editor.
const TARGET_OTHER_STORY: &str = refused!(
    "object.move-other-story",
    "An object can be moved within the text it is in, but not from the body into a \
     header, footer or note, or back. Drop it in the same part of the document."
);

/// The insertion at the drop point failed for a reason the operation did not
/// explain itself — an offset inside a link or a field result, say.
const TARGET_REFUSED: &str = refused!(
    "object.move-target-refused",
    "An object cannot be put at that exact spot, which is inside a link or a field. \
     Drop it just before or after it."
);

/// A copy of a chart, a diagram, an embedded object or a group would have to
/// share a package part or a projection with the original.
const COPY_UNSUPPORTED: &str = refused!(
    "object.copy-unsupported",
    "A chart, diagram, embedded object or group can be moved in the text but not copied \
     by dragging yet. Drag it without the copy key to move it."
);

/// A copy whose look comes from a node-keyed side table a fresh id cannot carry.
const COPY_LOSES_LOOK: &str = refused!(
    "object.copy-loses-look",
    "A copy of this object would lose the theme style or chart data it takes its look \
     from, so it can be moved but not copied by dragging yet."
);

#[wasm_bindgen]
impl WasmDocument {
    /// Moves the in-line object `node` to the caret position `(targetNode,
    /// targetOffset)` in the text — or, with `copy`, leaves it where it is and
    /// inserts a copy there — as ONE undoable action (Word's and Docs' drag of an
    /// in-line picture; Ctrl-drag copies). The object keeps its id, so it stays
    /// selected across the move.
    ///
    /// The result's caret is the drop position and, for a copy, `placedObject`
    /// names the new object. An object dropped back on its own position is
    /// reported as the document UNCHANGED — the same revision, no dirty pages,
    /// nothing on the undo stack — the way an empty review comparison or a
    /// no-op drop-cap removal already is, rather than as an empty edit that
    /// would mark the document changed.
    ///
    /// # Errors
    ///
    /// A marked refusal (`refused: <sentence>\u{1f}<code>`) when the object is not
    /// directly in the text (floating, or part of a link, field, content control
    /// or tracked change), when the drop point is not ordinary text or is in
    /// another story, when the insertion there is refused, or when a copy of this
    /// kind of object would lose part of what it is. The document is unchanged on
    /// every error.
    ///
    /// **O(document)**, once per gesture; see the module documentation.
    #[wasm_bindgen(js_name = moveInlineObject)]
    pub fn move_inline_object(
        &mut self,
        node: &str,
        target_node: &str,
        target_offset: u32,
        copy: bool,
    ) -> Result<EditResult, JsValue> {
        self.move_inline_object_inner(node, target_node, target_offset, copy)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// [`move_inline_object`](Self::move_inline_object) with a `String` error, so
    /// a native test can read the refusal.
    pub(crate) fn move_inline_object_inner(
        &mut self,
        node: &str,
        target_node: &str,
        target_offset: u32,
        copy: bool,
    ) -> Result<EditResult, String> {
        let object = node_id_msg(node)?;
        let target = node_id_msg(target_node)?;
        let Some(home) = casual_doc_edit::inline_object_position(&self.document, object) else {
            return Err(self.not_in_text_reason(object).to_owned());
        };
        // A floating object sits in its anchor paragraph's inline list too, and
        // moving THAT would move its anchor while it stayed where it was drawn —
        // a different edit (Word's anchor lock), not this one.
        if floats(home.node) {
            return Err(FLOATS_FREELY.to_owned());
        }
        let (home_paragraph, home_offset) = (home.paragraph, home.offset);
        let carried = home.node.clone();
        // Dropped back where it came from. An object is zero-width in offset
        // space, so the positions just before and just after it are both this
        // one; there is nothing to do and nothing to put on the undo stack. A
        // copy dropped there is treated as the same non-gesture — a drag that
        // ended where it began — rather than as a request for a second picture
        // in the same spot.
        if home_paragraph == target && home_offset == target_offset {
            return Ok(EditResult {
                node: target.to_string(),
                offset: target_offset,
                revision: self.revision,
                page_count: self.page_count(),
                dirty: Vec::new(),
                paste_loss: Vec::new(),
                placed_object: String::new(),
            });
        }
        // The drop must be a paragraph — in the body, a table cell or a text
        // box's own text — of the same story the object is in. `surface_of`
        // answers for paragraphs and for table structure, so the paragraph check
        // is what refuses a drop on a table, a row or a cell by name.
        let target_surface = casual_doc_edit::surface_of(&self.document, target)
            .filter(|_| casual_doc_edit::find_paragraph_any(&self.document, target).is_some())
            .ok_or_else(|| TARGET_NOT_TEXT.to_owned())?;
        if casual_doc_edit::surface_of(&self.document, home_paragraph) != Some(target_surface) {
            return Err(TARGET_OTHER_STORY.to_owned());
        }
        // A text box dropped into its own text would be removed together with
        // the paragraph it is meant to land in. O(subtree).
        if node_ids_of_inline(&carried).contains(&target) {
            return Err(TARGET_INSIDE_ITSELF.to_owned());
        }
        let at = Pos::new(target, target_offset);
        let mut report = CloneReport::default();
        let (operations, placed, kind) = if copy {
            let duplicate = self.copy_for_drop(&carried, &mut report)?;
            let placed = duplicate.id();
            (
                vec![Operation::InsertInlineObject {
                    at,
                    node: Box::new(duplicate),
                }],
                Some(placed),
                HistoryKind::ObjectCopy,
            )
        } else {
            (
                // The removal first, so the insertion re-uses an id that is no
                // longer in the document. The object is zero-width, so removing it
                // shifts no offset and `at` means the same place afterwards.
                vec![
                    Operation::RemoveInlineObject { object },
                    Operation::InsertInlineObject {
                        at,
                        node: Box::new(carried),
                    },
                ],
                None,
                HistoryKind::ObjectMove,
            )
        };
        let mut result = self
            .apply_action_caret_as(operations, at, kind)
            .map_err(|error| {
                // A refusal the operation already wrote for the reader (a
                // protected range, say) passes through; anything else is internal
                // vocabulary, and the sentence that fits it is about the spot.
                if error.starts_with(casual_doc_edit::refusal::MARKER) {
                    error
                } else {
                    TARGET_REFUSED.to_owned()
                }
            })?;
        result.placed_object = placed.map(|id| id.to_string()).unwrap_or_default();
        // A text box's own content can hold the per-range markers a copy cannot
        // duplicate; they are reported the way a paste reports them.
        result.paste_loss = report.kinds().into_iter().map(str::to_owned).collect();
        Ok(result)
    }

    /// Why `object` cannot move in the text, given that it is not directly in a
    /// paragraph's inline list. **O(document)**, one walk.
    fn not_in_text_reason(&self, object: NodeId) -> &'static str {
        if object_anchor_any_surface(&self.document, object).is_some() {
            FLOATS_FREELY
        } else {
            NOT_DIRECTLY_IN_TEXT
        }
    }

    /// The copy a Ctrl-drag drops: the clipboard's total copy with fresh ids, or
    /// a refusal when the copy could not be what the original is.
    ///
    /// **O(subtree)** for the id list, plus O(charts) for the projection check.
    fn copy_for_drop(
        &mut self,
        original: &InlineNode,
        report: &mut CloneReport,
    ) -> Result<InlineNode, String> {
        // A picture shares its media entry, which is immutable, and a text box is
        // self-contained apart from what the side-table check below looks for.
        // Everything else is refused, including any kind the model gains later,
        // which is the safe default for a copy: a chart's data is a projection
        // keyed by its object id, an OLE object or diagram is a package part the
        // two would share, and a group's members carry theme styles by id.
        if !matches!(original, InlineNode::Drawing(_) | InlineNode::TextBox(_)) {
            return Err(COPY_UNSUPPORTED.to_owned());
        }
        let ids = node_ids_of_inline(original);
        let definitions = self.document.definitions();
        let keyed_look = ids
            .iter()
            .any(|id| definitions.shape_styles.contains_key(id))
            || definitions
                .charts
                .iter()
                .any(|(_, chart)| ids.contains(&chart.object));
        if keyed_look {
            return Err(COPY_LOSES_LOOK.to_owned());
        }
        clone_inline_with_fresh_ids(original, &mut self.edit_ids, report)
            .ok_or_else(|| "id space exhausted".to_owned())
    }
}

/// Whether `node` is a floating carrier — drawn where its anchor says rather
/// than where it sits in the text. O(1).
fn floats(node: &InlineNode) -> bool {
    match node {
        InlineNode::AnchoredDrawing(_) => true,
        InlineNode::TextBox(text_box) => text_box.anchor.is_some(),
        InlineNode::Group(group) => group.anchor.is_some(),
        _ => false,
    }
}

#[cfg(test)]
#[path = "inline_move_tests.rs"]
mod tests;
