// SPDX-License-Identifier: Apache-2.0

//! The document's Track Changes setting (`w:trackRevisions`, `docs/165` M7,
//! `docs/109` HF-282): read from the file so the host can open the document in
//! Suggesting, written by one undoable edit when the reader switches, and saved
//! with the document — so a reopen honours it.

use crate::{DocxPackage, WasmDocument, open_document, viewer_limits};

const SAMPLE_DOCX: &[u8] = include_bytes!("../../../webapp/sample.docx");
const DOCX: &str = casual_doc_io::formats::DOCX;

fn settings_xml(d: &mut WasmDocument) -> String {
    let bytes = d
        .export_as_inner(DOCX, "preserve_when_safe")
        .expect("the save exports")
        .bytes;
    let mut package = DocxPackage::open(&bytes, viewer_limits()).expect("open package");
    String::from_utf8(
        package
            .read_part("word/settings.xml")
            .expect("read settings"),
    )
    .unwrap()
}

fn saved_and_reopened(d: &mut WasmDocument) -> WasmDocument {
    let bytes = d
        .export_as_inner(DOCX, "preserve_when_safe")
        .expect("the save exports")
        .bytes;
    open_document(&bytes).expect("the saved file reopens")
}

#[test]
fn turning_track_changes_on_is_one_undoable_edit_that_a_save_writes_and_a_reopen_honours() {
    let mut d = open_document(SAMPLE_DOCX).expect("open sample.docx");
    assert!(!d.track_revisions(), "sample.docx does not track changes");
    assert!(!settings_xml(&mut d).contains("<w:trackRevisions"));
    let before = d.revision;

    let result = d
        .set_track_revisions_inner(true, "", 0)
        .expect("turn tracking on");
    assert_eq!(result.revision, before + 1, "one edit");
    assert!(d.track_revisions());
    assert_eq!(d.undo_label(), "Track changes", "and it says what it was");

    assert!(
        settings_xml(&mut d).contains("<w:trackRevisions/>"),
        "the save writes the setting"
    );
    let reopened = saved_and_reopened(&mut d);
    assert!(
        reopened.track_revisions(),
        "and the saved file opens with tracking on"
    );

    // Asking for what the document already says is no edit at all.
    let same = d.set_track_revisions_inner(true, "", 0).expect("ask again");
    assert_eq!(same.revision, d.revision, "nothing changed");
    assert_eq!(d.undo_label(), "Track changes");

    // One Undo puts it back, and the next save says so.
    d.undo_inner().expect("undo");
    assert!(!d.track_revisions());
    assert!(!settings_xml(&mut d).contains("<w:trackRevisions"));
    assert!(!saved_and_reopened(&mut d).track_revisions());
}

/// A `trackedChanges` restriction "shall imply the presence of the
/// `trackRevisions` element, and applications shall not allow that element's
/// state to be changed to false" (ECMA-376 §17.15.1.29): under it, tracking can
/// be turned on and cannot be turned off.
#[test]
fn a_tracked_changes_restriction_lets_tracking_on_and_never_off() {
    let mut d = open_document(SAMPLE_DOCX).expect("open sample.docx");
    d.set_document_protection_inner(Some("trackedChanges"), true, false)
        .expect("restrict editing to tracked changes");
    d.set_track_revisions_inner(true, "", 0)
        .expect("turning tracking on is what the restriction asks for");
    assert!(d.track_revisions());
    let refusal = d
        .set_track_revisions_inner(false, "", 0)
        .expect_err("turning tracking off is refused");
    assert!(!refusal.is_empty());
    assert!(d.track_revisions(), "and it stays on");
}
