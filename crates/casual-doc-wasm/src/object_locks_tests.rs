// SPDX-License-Identifier: Apache-2.0

//! Word's "Lock aspect ratio" (`docs/109` FID-AT-09), asserted where the host
//! reads it: the `locksAspectRatio` an object's selection carries, which the
//! resize grips turn into a proportional or a free corner drag.
//!
//! Word honours the file's `noChangeAspect` both ways, so these hold both
//! directions — a lock the file states is published, a picture whose file states
//! none is NOT — and the one this editor writes itself: a picture inserted here
//! carries Word's lock, through a save and an undo.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{LockFlags, ObjectLocks};
use serde_json::Value;

use crate::{DocxPackage, WasmDocument, open_document, viewer_limits};

const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");
const FLOAT_DOCX: &[u8] = include_bytes!("../../../webapp/float.docx");
const SHAPES_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/shapes.docx");

/// The `objectOrder` entries, as the host receives them.
fn order(d: &WasmDocument) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(&d.object_order()).expect("objectOrder is a JSON list")
}

fn locked(entry: &Value) -> bool {
    entry["locksAspectRatio"]
        .as_bool()
        .expect("every entry states the lock, true or false")
}

fn node(entry: &Value, key: &str) -> NodeId {
    entry[key]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a node id")
}

/// One part of a written package, as text.
fn part_text(bytes: &[u8], part: &str) -> String {
    let mut package = DocxPackage::open(bytes, viewer_limits()).expect("open package");
    String::from_utf8(package.read_part(part).expect("read part")).expect("utf-8")
}

fn aspect(frame: bool, object: bool) -> ObjectLocks {
    ObjectLocks {
        frame: LockFlags {
            no_change_aspect: frame,
            ..LockFlags::default()
        },
        object: LockFlags {
            no_change_aspect: object,
            ..LockFlags::default()
        },
    }
}

#[test]
fn a_picture_publishes_the_aspect_lock_its_file_states_and_none_when_it_states_none() {
    // sample.docx is Word's: `a:graphicFrameLocks noChangeAspect="1"` on both
    // pictures' frames.
    let sample = open_document(SAMPLE_DOCX).expect("open sample.docx");
    let pictures: Vec<Value> = order(&sample)
        .into_iter()
        .filter(|entry| entry["kind"] == "image")
        .collect();
    assert_eq!(pictures.len(), 2, "sample.docx has two pictures");
    assert!(
        pictures.iter().all(locked),
        "Word locked both pictures' aspect ratio, and the selection says so: {pictures:?}"
    );

    // float.docx's picture states no lock at all. Word reads that as unlocked —
    // a corner drag stretches it — and so does the selection.
    let float = open_document(FLOAT_DOCX).expect("open float.docx");
    let entries = order(&float);
    let picture = entries
        .iter()
        .find(|entry| entry["kind"] == "image")
        .expect("float.docx has a picture");
    assert!(
        !locked(picture),
        "an absent noChangeAspect is unlocked, whatever the kind"
    );

    // The hit test answers the same thing the list does.
    let rect = float.object_rect(picture["node"].as_str().unwrap());
    let hit = float
        .object_at(rect[0] as u32, rect[1] + rect[3] / 2, rect[2] + rect[4] / 2)
        .expect("a click on the picture hits it");
    assert!(!hit.locks_aspect_ratio());
    let rect = sample.object_rect(pictures[0]["node"].as_str().unwrap());
    let hit = sample
        .object_at(rect[0] as u32, rect[1] + rect[3] / 2, rect[2] + rect[4] / 2)
        .expect("a click on the picture hits it");
    assert!(hit.locks_aspect_ratio());
}

/// A lone shape is a group of one in the model, and a Word file writes its lock
/// on the SHAPE (`wps:cNvSpPr/a:spLocks`), which the importer keys by the shape —
/// the selection's subject, not its root. A many-member group's member does not
/// lend the group its lock, because the group is what a drag would resize.
#[test]
fn a_lone_shape_whose_file_locks_its_aspect_publishes_locked() {
    let mut d = open_document(SHAPES_DOCX).expect("open shapes.docx");
    let entries = order(&d);
    let lone = entries
        .iter()
        .find(|entry| entry["kind"] == "shape" && entry["subject"] != entry["root"])
        .expect("shapes.docx opens with a lone rectangle, a group of one");
    let shape = node(lone, "subject");
    assert!(!locked(lone), "the fixture's rectangle states no lock");

    // What the importer records for `<a:spLocks noChangeAspect="1"/>`, written
    // through the real writer and read back through the real importer, so the
    // assertion is about a FILE that says so.
    d.document.definitions_mut().object_names.insert(
        shape,
        casual_doc_model::v1::ObjectName {
            locks: aspect(false, true),
            ..Default::default()
        },
    );
    let bytes = d.export_docx().expect("export");
    assert!(
        part_text(&bytes, "word/document.xml").contains(r#"<a:spLocks noChangeAspect="1"/>"#),
        "the file states the shape's lock where Word writes it"
    );
    let reopened = open_document(&bytes).expect("reopen");
    let entries = order(&reopened);
    let lone = entries
        .iter()
        .find(|entry| entry["kind"] == "shape" && entry["subject"] != entry["root"])
        .expect("still a lone shape");
    assert!(
        locked(lone),
        "a shape whose file locks its ratio keeps it on a corner drag: {lone:?}"
    );

    // A member of the many-member group, locked: the group (its root, which is
    // what a resize acts on) is not — neither as a whole nor through the member
    // a double-click descends to.
    let group = entries
        .iter()
        .find(|entry| entry["subject"] == entry["root"] && entry["kind"] != "image")
        .map(|entry| node(entry, "root"))
        .expect("shapes.docx also has a many-member group, selected as a unit");
    let members: Vec<Value> =
        serde_json::from_str(&reopened.object_descendants(&group.to_string())).unwrap();
    assert!(
        members.len() > 1,
        "the group has several members: {members:?}"
    );
    let member = node(&members[0], "subject");
    let mut d = reopened;
    d.document.definitions_mut().object_names.insert(
        member,
        casual_doc_model::v1::ObjectName {
            locks: aspect(false, true),
            ..Default::default()
        },
    );
    let members: Vec<Value> =
        serde_json::from_str(&d.object_descendants(&group.to_string())).unwrap();
    assert!(
        !locked(&members[0]),
        "a member's lock does not lock the group a drag would resize"
    );
    let whole = order(&d)
        .into_iter()
        .find(|entry| node(entry, "root") == group)
        .expect("the group is still listed");
    assert!(!locked(&whole));
}

/// Word writes `a:graphicFrameLocks noChangeAspect="1"` on every picture it
/// inserts, and so does Insert ▸ Picture here — or an inserted picture would
/// stretch on a corner drag the moment absent became unlocked, and in Word
/// after a save.
#[test]
fn an_inserted_picture_carries_words_aspect_lock_through_a_save_and_an_undo() {
    // 1×1 PNG.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let mut d = open_document(FLOAT_DOCX).expect("open float.docx");
    let before: Vec<String> = order(&d)
        .iter()
        .map(|entry| entry["node"].as_str().unwrap().to_owned())
        .collect();
    let paragraph = crate::block_node_id(&d.document.body()[0]).to_string();
    d.insert_image(
        &paragraph,
        0,
        PNG_1X1.to_vec(),
        1_828_800.0,
        914_400.0,
        "image/png".to_owned(),
    )
    .expect("insert a picture");
    let entries = order(&d);
    let inserted = entries
        .iter()
        .find(|entry| !before.contains(&entry["node"].as_str().unwrap().to_owned()))
        .expect("the inserted picture is selectable");
    let picture = node(inserted, "node");
    assert!(
        locked(inserted),
        "an inserted picture keeps its proportions on a corner drag"
    );
    assert_eq!(d.undo_label(), "Insert image", "still one insert to undo");

    let bytes = d.export_docx().expect("export");
    let xml = part_text(&bytes, "word/document.xml");
    assert_eq!(
        xml.matches(r#"noChangeAspect="1""#).count(),
        1,
        "the save writes Word's frame lock on the inserted picture, and only there \
         (float.docx's own picture states none)"
    );
    let reopened = open_document(&bytes).expect("reopen");
    assert_eq!(
        order(&reopened)
            .iter()
            .filter(|entry| locked(entry))
            .count(),
        1,
        "and Word — or this editor — reading it back finds it locked"
    );

    // One Undo removes the picture and its lock together: no orphan entry.
    d.undo().expect("undo the insert");
    assert!(
        d.document
            .definitions()
            .object_names
            .get(&picture)
            .is_none(),
        "the side-table entry went with the picture"
    );
    d.redo().expect("redo the insert");
    assert!(d.document.definitions().locks_aspect_ratio(picture));
}
