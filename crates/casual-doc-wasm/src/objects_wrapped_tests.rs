// SPDX-License-Identifier: Apache-2.0

//! Pictures in the containers Word really puts them in, driven through the real
//! open path (`fixtures/generated/wrapped-pictures.docx`, built by
//! `fixtures/tools/make_wrapped_pictures.py`).
//!
//! Every assertion here is about a picture that painted and then refused the
//! click or the command aimed at it: a floating logo inside an `INCLUDEPICTURE`
//! field result or an inline content control (`docs/109` HF-166, HF-214), a
//! picture that is a member of a group (HF-214), a picture whose image had to be
//! swapped (HF-252) or whose border had to be set (HF-254).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, GroupChild, InlineNode};

use crate::{WasmDocument, open_document};

const WRAPPED: &[u8] = include_bytes!("../../../fixtures/generated/wrapped-pictures.docx");

fn wrapped() -> WasmDocument {
    open_document(WRAPPED).expect("open the wrapped-pictures fixture")
}

/// Every placed object, as the host's `objectOrder` reads it.
fn order(document: &WasmDocument) -> Vec<serde_json::Value> {
    serde_json::from_str(&document.object_order()).expect("objectOrder is JSON")
}

/// The id of the picture whose alt text is `descr`, wherever it is nested —
/// found by walking the model rather than through any API under test.
fn picture_named(document: &WasmDocument, descr: &str) -> NodeId {
    fn in_children(children: &[GroupChild], descr: &str) -> Option<NodeId> {
        children.iter().find_map(|child| match child {
            GroupChild::Picture(picture) if picture.descr.as_deref() == Some(descr) => {
                Some(picture.id)
            }
            GroupChild::Group(group) => in_children(&group.children, descr),
            _ => None,
        })
    }
    fn in_inlines(inlines: &[InlineNode], descr: &str) -> Option<NodeId> {
        inlines.iter().find_map(|inline| match inline {
            InlineNode::Drawing(drawing) if drawing.descr.as_deref() == Some(descr) => {
                Some(drawing.id)
            }
            InlineNode::AnchoredDrawing(drawing) if drawing.descr.as_deref() == Some(descr) => {
                Some(drawing.id)
            }
            InlineNode::Group(group) => in_children(&group.children, descr),
            other => crate::contained_inlines(other).and_then(|nested| in_inlines(nested, descr)),
        })
    }
    document
        .document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => in_inlines(&paragraph.inlines, descr),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the fixture has a picture described {descr:?}"))
}

/// The middle of `node`'s placed frame, as `(page, x, y)` in page twips.
fn centre(document: &WasmDocument, node: &str) -> (u32, i32, i32) {
    let rect = document.object_rect(node);
    assert_eq!(rect.len(), 5, "{node} is placed");
    (
        u32::try_from(rect[0]).expect("a page number"),
        rect[1] + rect[3] / 2,
        rect[2] + rect[4] / 2,
    )
}

/// HF-166 / HF-214: a floating picture inside a field result, and one inside an
/// inline content control, are objects — a click on them selects them, and the
/// commands a selected float offers all reach them.
///
/// Before the fix both painted and a click on either dropped a caret into the
/// paragraph behind: `object_anchor_in_inlines` named `Hyperlink` and
/// `Revision` and stopped behind a wildcard, so `resolve_object_boxes` decided
/// these floats had no anchor and skipped them.
#[test]
fn a_float_in_a_field_result_or_a_content_control_is_selectable_and_editable() {
    let mut document = wrapped();
    for descr in ["Field logo", "Control logo"] {
        let node = picture_named(&document, descr).to_string();
        let (page, x, y) = centre(&document, &node);
        let hit = document
            .object_at(page, x, y)
            .unwrap_or_else(|| panic!("a click on the {descr} selects it"));
        assert_eq!(hit.subject(), node, "{descr}: the click names the picture");
        assert!(hit.anchored, "{descr}: it is reported as floating");
        for (name, can) in [
            ("canMove", hit.can_move()),
            ("canWrap", hit.can_wrap()),
            ("canResize", hit.can_resize()),
            ("canDelete", hit.can_delete()),
            ("canAltText", hit.can_alt_text()),
            ("canCrop", hit.can_crop()),
        ] {
            assert!(can, "{descr}: {name}");
        }
        assert!(
            order(&document)
                .iter()
                .any(|entry| entry["subject"] == serde_json::json!(node)),
            "{descr}: Tab traversal reaches it too"
        );

        // Move, wrap and undo reach the object inside its container.
        let before = document.object_rect(&node);
        document
            .set_object_anchor_position(&node, 3_000_000.0, 3_000_000.0)
            .expect("move it");
        assert_ne!(document.object_rect(&node), before, "{descr}: it moved");
        document.undo().expect("undo the move");
        assert_eq!(
            document.object_rect(&node),
            before,
            "{descr}: undo restores it"
        );
        document
            .set_object_wrap(&node, "square")
            .expect("re-wrap it");
        assert_eq!(document.object_wrap(&node), "square", "{descr}");
        document
            .set_object_descr(&node, Some(format!("{descr} (edited)")))
            .expect("describe it");
        assert_eq!(
            document.object_descr(&node).as_deref(),
            Some(format!("{descr} (edited)").as_str())
        );
    }
}

/// HF-214: a picture that is a member of a group is croppable and describable,
/// and says so — the capability bits come from the same carrier facts as the
/// commands that now accept it.
#[test]
fn a_grouped_picture_can_be_cropped_described_and_bordered() {
    let mut document = wrapped();
    let child = picture_named(&document, "Partner B").to_string();
    let root = order(&document)
        .iter()
        .find(|entry| entry["kind"] == "group")
        .expect("the partner logos are a group")["root"]
        .as_str()
        .expect("a root id")
        .to_owned();
    let descendants: Vec<serde_json::Value> =
        serde_json::from_str(&document.object_descendants(&root)).expect("descendants are JSON");
    let entry = descendants
        .iter()
        .find(|entry| entry["subject"] == serde_json::json!(child))
        .expect("the grouped picture is a descendant");
    for name in ["canAltText", "canCrop", "canStroke"] {
        assert_eq!(entry[name], true, "a grouped picture publishes {name}");
        assert!(
            entry["capabilityReasons"].get(name).is_none(),
            "and no refusal for {name}"
        );
    }

    document
        .set_image_crop(&child, Some(vec![0.25, 0.0, 0.0, 0.0]))
        .expect("crop a grouped picture");
    let crop = document.object_crop(&child).expect("its crop reads back");
    assert!((crop[0] - 0.25).abs() < 1e-3, "left inset {crop:?}");
    document
        .set_object_descr(&child, Some("Second partner".to_owned()))
        .expect("describe a grouped picture");
    assert_eq!(
        document.object_descr(&child).as_deref(),
        Some("Second partner")
    );
    document
        .set_shape_outline(&child, Some("#cc0000".to_owned()), Some(25_400.0))
        .expect("border a grouped picture");
    let format = document
        .shape_format(&child)
        .expect("a format read")
        .expect("a grouped picture reports its border");
    assert_eq!(format.outline.as_deref(), Some("#cc0000"));
    assert_eq!(format.fill, None, "a picture has no fill to report");

    // The three survive a save, because each is the picture's own field.
    let reopened = open_document(&document.export_docx().expect("export")).expect("reopen");
    let child = picture_named(&reopened, "Second partner").to_string();
    let crop = reopened.object_crop(&child).expect("crop survives");
    assert!((crop[0] - 0.25).abs() < 1e-3, "{crop:?}");
    assert_eq!(
        reopened
            .shape_format(&child)
            .expect("read")
            .and_then(|format| format.outline)
            .as_deref(),
        Some("#cc0000")
    );
}

/// HF-214's "removed", which was worse than unreachable: Delete on a picture
/// selected inside a group went to the selection's ROOT, so deleting one of two
/// grouped logos deleted both. Deleting a member removes that member only; the
/// last member takes the group with it; one undo restores either.
#[test]
fn deleting_a_grouped_picture_deletes_that_picture_and_not_its_group() {
    let mut document = wrapped();
    let b = picture_named(&document, "Partner B").to_string();
    let a = picture_named(&document, "Partner A").to_string();
    let a_rect = document.object_rect(&a);
    document
        .delete_group_member_inner(&b)
        .expect("delete one member");
    assert_eq!(
        document.object_rect(&b),
        Vec::<i32>::new(),
        "the deleted picture is gone"
    );
    assert_eq!(
        document.object_rect(&a),
        a_rect,
        "the other member stays, exactly where it was"
    );
    document.undo().expect("undo");
    assert_eq!(document.object_rect(&b).len(), 5, "undo restores it");

    // The last member takes the group with it.
    document.delete_group_member_inner(&b).expect("B again");
    document.delete_group_member_inner(&a).expect("then A");
    assert!(
        !order(&document)
            .iter()
            .any(|entry| entry["kind"] == "group"),
        "an emptied group is not left behind as an invisible object"
    );

    // A top-level object is not a group member, and says so.
    let loose = picture_named(&document, "Field logo").to_string();
    let refused = document
        .delete_group_member_inner(&loose)
        .expect_err("not a member");
    assert!(refused.starts_with("refused: "), "{refused}");
}

/// HF-254: Word's Picture Border, on a loose picture — set, recoloured without
/// losing its weight, removed, and undone, all through the one stroke operation.
#[test]
fn a_picture_border_is_set_recoloured_removed_and_undone() {
    let mut document = wrapped();
    let node = picture_named(&document, "Field logo").to_string();
    let none = document
        .shape_format(&node)
        .expect("read")
        .expect("a picture");
    assert_eq!(
        none.outline, None,
        "the fixture's picture starts unbordered"
    );

    document
        .set_shape_outline(&node, Some("#3355c4".to_owned()), Some(38_100.0))
        .expect("add a border");
    document
        .set_shape_outline(&node, Some("#00aa00".to_owned()), None)
        .expect("recolour it");
    let format = document
        .shape_format(&node)
        .expect("read")
        .expect("a picture");
    assert_eq!(format.outline.as_deref(), Some("#00aa00"));
    assert_eq!(
        format.outline_width_emu,
        Some(38_100.0),
        "a colour change keeps the weight the border already had"
    );
    document
        .set_shape_outline(&node, None, None)
        .expect("remove it");
    assert_eq!(
        document
            .shape_format(&node)
            .expect("read")
            .expect("a picture")
            .outline,
        None
    );
    document.undo().expect("undo the removal");
    assert_eq!(
        document
            .shape_format(&node)
            .expect("read")
            .expect("a picture")
            .outline
            .as_deref(),
        Some("#00aa00"),
        "undo puts the border back"
    );
}

/// HF-252: Change Picture keeps the OBJECT and swaps the image — position,
/// wrap, alt text and border survive; the new image is fitted inside the old
/// frame at its own proportions; the crop (a fraction of the old pixels) goes;
/// one undo restores the old image exactly.
#[test]
fn change_picture_keeps_the_object_and_fits_the_new_image_in_its_frame() {
    let mut document = wrapped();
    let node = picture_named(&document, "Field logo").to_string();
    document
        .set_image_crop(&node, Some(vec![0.1, 0.1, 0.1, 0.1]))
        .expect("crop it first");
    document
        .set_shape_outline(&node, Some("#112233".to_owned()), Some(12_700.0))
        .expect("border it first");
    document
        .set_object_wrap(&node, "square")
        .expect("wrap it first");
    let before_rect = document.object_rect(&node);
    let before_extent = document.object_extent(&node);
    let before_media = media_of(&document, &node);

    // A PORTRAIT image (1:2) into a LANDSCAPE frame (2:1): it must fit the
    // frame's height and be half as wide as tall, not be stretched to 2:1.
    document
        .replace_picture_inner(&node, vec![1, 2, 3], 457_200.0, 914_400.0, "image/png")
        .expect("change the picture");

    assert_ne!(
        media_of(&document, &node),
        before_media,
        "the image changed"
    );
    let extent = document.object_extent(&node);
    assert!(
        (extent[1] - before_extent[1]).abs() < 1.0,
        "the new image fills the frame's height: {extent:?} in {before_extent:?}"
    );
    assert!(
        (extent[0] * 2.0 - extent[1]).abs() < 2.0,
        "at its own 1:2 proportions, not the old frame's: {extent:?}"
    );
    let rect = document.object_rect(&node);
    assert_eq!(
        (rect[1], rect[2]),
        (before_rect[1], before_rect[2]),
        "the picture stays where it was"
    );
    assert_eq!(document.object_wrap(&node), "square", "the wrap is kept");
    assert_eq!(
        document.object_descr(&node).as_deref(),
        Some("Field logo"),
        "the alt text is kept"
    );
    assert_eq!(
        document
            .shape_format(&node)
            .expect("read")
            .and_then(|format| format.outline)
            .as_deref(),
        Some("#112233"),
        "the border is kept"
    );
    assert_eq!(
        document.object_crop(&node),
        Some(vec![0.0; 4]),
        "the old image's crop does not carry over"
    );

    document.undo().expect("one undo");
    assert_eq!(
        media_of(&document, &node),
        before_media,
        "undo restores the image"
    );
    assert_eq!(document.object_extent(&node), before_extent);
    let crop = document.object_crop(&node).expect("crop");
    assert!((crop[0] - 0.1).abs() < 1e-3, "and its crop: {crop:?}");

    // The replaced image is a real part in the saved package.
    document.redo().expect("redo");
    let bytes = document.export_docx().expect("export");
    let reopened = open_document(&bytes).expect("reopen");
    let reopened_node = picture_named(&reopened, "Field logo").to_string();
    let part = reopened
        .document
        .definitions()
        .media
        .get(&media_of(&reopened, &reopened_node))
        .expect("the new media resolves")
        .part_name
        .clone();
    assert!(
        reopened.resources.get(&part) == Some([1_u8, 2, 3].as_slice()),
        "the saved file carries the new image's bytes at {part}"
    );
}

/// Change Picture on an INLINE picture inside a field result, and its refusals.
#[test]
fn change_picture_reaches_an_inline_picture_and_refuses_what_it_cannot_use() {
    let mut document = wrapped();
    let inline = picture_named(&document, "Inline logo").to_string();
    document
        .replace_picture_inner(&inline, vec![9], 914_400.0, 914_400.0, "image/jpeg")
        .expect("change an inline picture in a field result");
    let extent = document.object_extent(&inline);
    assert!(
        (extent[0] - extent[1]).abs() < 1.0,
        "a square image stays square: {extent:?}"
    );

    let unsupported = document
        .replace_picture_inner(&inline, vec![9], 10.0, 10.0, "application/pdf")
        .expect_err("a PDF is not a picture");
    assert!(unsupported.starts_with("refused: "), "{unsupported}");
    let empty = document
        .replace_picture_inner(&inline, Vec::new(), 10.0, 10.0, "image/png")
        .expect_err("empty bytes are refused");
    assert!(empty.starts_with("refused: "), "{empty}");
    let paragraph = document
        .document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
            _ => None,
        })
        .expect("a paragraph");
    let not_a_picture = document
        .replace_picture_inner(&paragraph, vec![9], 10.0, 10.0, "image/png")
        .expect_err("a paragraph is not a picture");
    assert!(not_a_picture.starts_with("refused: "), "{not_a_picture}");
}

/// The media id a picture node currently shows, wherever it is nested.
fn media_of(document: &WasmDocument, node: &str) -> casual_doc_model::v1::MediaId {
    fn in_children(children: &[GroupChild], id: NodeId) -> Option<casual_doc_model::v1::MediaId> {
        children.iter().find_map(|child| match child {
            GroupChild::Picture(picture) if picture.id == id => Some(picture.media),
            GroupChild::Group(group) => in_children(&group.children, id),
            _ => None,
        })
    }
    fn in_inlines(inlines: &[InlineNode], id: NodeId) -> Option<casual_doc_model::v1::MediaId> {
        inlines.iter().find_map(|inline| match inline {
            InlineNode::Drawing(drawing) if drawing.id == id => Some(drawing.media),
            InlineNode::AnchoredDrawing(drawing) if drawing.id == id => Some(drawing.media),
            InlineNode::Group(group) => in_children(&group.children, id),
            other => crate::contained_inlines(other).and_then(|nested| in_inlines(nested, id)),
        })
    }
    let id: NodeId = node.parse().expect("a node id");
    document
        .document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => in_inlines(&paragraph.inlines, id),
            _ => None,
        })
        .expect("the picture is in the body")
}
