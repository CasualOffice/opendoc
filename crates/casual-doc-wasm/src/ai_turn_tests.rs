// SPDX-License-Identifier: Apache-2.0

//! The Casual AI co-editing turn (`ai_turn.rs`, `docs/183` §23): one undo step, the person's
//! interleaved edits spared, the anchor mapped, suggestions as ONE revision, tracked tables.

use std::collections::BTreeMap;

use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, RevisionKind,
    Run, RunProperties,
};

use crate::{WasmDocument, open_document, write_document};

const DATE: &str = "2026-10-31T12:00:00Z";

fn docx(texts: &[&str]) -> Vec<u8> {
    let id = |n: u64| NodeId::from_parts(n, 208).unwrap();
    let blocks = texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let n = 10 * (index as u64 + 1);
            BlockNode::Paragraph(Paragraph {
                id: id(n),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: id(n + 1),
                    properties: RunProperties::default().into(),
                    text: (*text).to_owned(),
                })],
            })
        })
        .collect();
    let document = Document::new(id(1), blocks, Definitions::default()).expect("valid");
    write_document(&document, &BTreeMap::new()).expect("write")
}

fn open(texts: &[&str]) -> WasmDocument {
    open_document(&docx(texts)).expect("open")
}

/// What a reader sees with every suggestion shown as if accepted (deletions hidden).
fn visible(inlines: &[InlineNode], out: &mut String) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Revision(revision) if revision.kind != RevisionKind::Deletion => {
                visible(&revision.inlines, out);
            }
            _ => {}
        }
    }
}

/// Every body paragraph's visible text, tables flattened as `[a|b]`.
fn texts(d: &WasmDocument) -> Vec<String> {
    d.document
        .body()
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                let mut out = String::new();
                visible(&paragraph.inlines, &mut out);
                out
            }
            BlockNode::Table(table) => {
                let cells: Vec<String> = table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .map(|cell| {
                        let mut out = String::new();
                        for block in &cell.blocks {
                            if let BlockNode::Paragraph(paragraph) = block {
                                visible(&paragraph.inlines, &mut out);
                            }
                        }
                        out
                    })
                    .collect();
                format!("[{}]", cells.join("|"))
            }
            _ => String::new(),
        })
        .collect()
}

fn ids(d: &WasmDocument) -> Vec<String> {
    d.document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
            _ => None,
        })
        .collect()
}

fn items(d: &WasmDocument) -> Vec<serde_json::Value> {
    serde_json::from_str(&d.list_revisions()).expect("revision list")
}

/// The person typing, through the editor's own typing path (session 7).
fn person_types(d: &mut WasmDocument, node: &str, offset: u32, text: &str) {
    if d.type_text(node, offset, node, offset, text.to_owned(), 7)
        .is_err()
    {
        panic!("the person's typing applies");
    }
}

fn undo(d: &mut WasmDocument) {
    d.undo_inner().expect("undo");
}

#[test]
fn a_streamed_turn_is_one_undo_step_that_restores_the_document_exactly() {
    let mut d = open(&["Alpha", "Beta"]);
    let p = ids(&d);
    let before = texts(&d);
    let turn = d.ai_turn_begin_inner("direct", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(turn, &p[0], 5).expect("anchor");
    for batch in [" is", " the first", " letter.", "\nA new", " paragraph."] {
        d.ai_turn_write_inner(turn, batch).expect("write");
    }
    assert_eq!(
        texts(&d),
        ["Alpha is the first letter.", "A new paragraph.", "Beta"]
    );
    d.ai_turn_end_value(turn).expect("end");
    assert_eq!(d.undo_label(), "Casual AI", "the step says who wrote it");
    undo(&mut d);
    assert_eq!(texts(&d), before, "ONE undo takes the whole turn back");
    assert!(
        d.log.undo_target().is_none(),
        "and nothing else was recorded"
    );
}

#[test]
fn the_persons_edit_between_two_writes_survives_reject_and_undo_and_stays_their_own_step() {
    for revert in [true, false] {
        let mut d = open(&["Alpha", "Beta"]);
        let p = ids(&d);
        let turn = d.ai_turn_begin_inner("direct", DATE).expect("begin");
        d.ai_turn_set_anchor_inner(turn, &p[0], 5).expect("anchor");
        d.ai_turn_write_inner(turn, " one").expect("write");
        person_types(&mut d, &p[1], 4, "!");
        person_types(&mut d, &p[1], 5, "?");
        d.ai_turn_write_inner(turn, " two").expect("write");
        assert_eq!(texts(&d), ["Alpha one two", "Beta!?"]);
        if revert {
            d.ai_turn_revert_inner(turn, false).expect("reject all");
            assert_eq!(
                texts(&d),
                ["Alpha", "Beta!?"],
                "Reject all spares the person"
            );
            undo(&mut d);
            assert_eq!(
                texts(&d),
                ["Alpha one two", "Beta!?"],
                "Reject all is undoable"
            );
        } else {
            d.ai_turn_end_value(turn).expect("end");
            // Newest occurrence first: the turn's last write is newer than "!?".
            undo(&mut d);
            assert_eq!(
                texts(&d),
                ["Alpha", "Beta!?"],
                "undo of the turn spares the person"
            );
            undo(&mut d);
            assert_eq!(
                texts(&d),
                ["Alpha", "Beta"],
                "the person's \"!?\" is ONE step of their own, not split by the AI's write"
            );
        }
    }
}

#[test]
fn the_persons_typing_in_front_of_the_anchor_moves_it() {
    let mut d = open(&["Alpha"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("direct", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(turn, &p[0], 5).expect("anchor");
    d.ai_turn_write_inner(turn, " AI").expect("write");
    person_types(&mut d, &p[0], 0, "My ");
    let anchor: serde_json::Value =
        serde_json::from_value(d.ai_turn_anchor_value(turn).expect("anchor")).expect("json");
    assert_eq!(anchor["offset"], 11, "mapped past the person's three bytes");
    d.ai_turn_write_inner(turn, " writes").expect("write");
    assert_eq!(texts(&d), ["My Alpha AI writes"]);
}

#[test]
fn a_safety_rollback_leaves_no_undo_entry_and_keeps_the_persons_edit() {
    let mut d = open(&["Alpha", "Beta"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(turn, &p[0], 5).expect("anchor");
    d.ai_turn_write_inner(turn, " bad").expect("write");
    person_types(&mut d, &p[1], 4, "!");
    d.ai_turn_write_inner(turn, " words").expect("write");
    d.ai_turn_revert_inner(turn, true).expect("rollback");
    assert_eq!(texts(&d), ["Alpha", "Beta!"]);
    assert!(items(&d).is_empty(), "no suggestion is left behind");
    // The only step left is the person's own.
    undo(&mut d);
    assert_eq!(texts(&d), ["Alpha", "Beta"]);
    assert!(
        d.log.undo_target().is_none(),
        "neither the turn nor its rollback is offered by Undo"
    );
}

#[test]
fn a_suggested_stream_is_one_revision_and_accept_all_is_one_step() {
    let mut d = open(&["Alpha"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(turn, &p[0], 5).expect("anchor");
    for batch in [" is", " the", " first."] {
        d.ai_turn_write_inner(turn, batch).expect("write");
    }
    let listed = items(&d);
    assert_eq!(
        listed.len(),
        1,
        "one streamed answer is ONE suggestion: {listed:?}"
    );
    assert_eq!(listed[0]["kind"], "insertion");
    assert_eq!(listed[0]["author"], "Casual AI");
    assert_eq!(listed[0]["text"], " is the first.");
    let end: serde_json::Value = d.ai_turn_end_value(turn).expect("end");
    assert_eq!(end["revisions"].as_array().map(Vec::len), Some(1));
    d.ai_turn_accept_inner(turn).expect("accept all");
    assert!(items(&d).is_empty(), "accepted");
    assert_eq!(texts(&d), ["Alpha is the first."]);
    undo(&mut d);
    assert_eq!(items(&d).len(), 1, "ONE undo step un-accepts it");
}

#[test]
fn a_replacement_strikes_the_old_text_and_writes_beside_it() {
    let mut d = open(&["Hello world"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_replace_inner(turn, &p[0], 6, 11)
        .expect("replace");
    d.ai_turn_write_inner(turn, "there").expect("write");
    assert_eq!(texts(&d), ["Hello there"]);
    let kinds: Vec<_> = items(&d).iter().map(|i| i["kind"].clone()).collect();
    assert_eq!(
        kinds,
        ["insertion", "deletion"],
        "the new words and the struck old ones"
    );
    d.ai_turn_revert_inner(turn, false).expect("reject all");
    assert_eq!(texts(&d), ["Hello world"]);
    assert!(items(&d).is_empty());
}

#[test]
fn a_suggested_table_is_one_card_that_reject_removes_and_accept_keeps() {
    for accept in [true, false] {
        let mut d = open(&["Intro", "After"]);
        let p = ids(&d);
        let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
        d.ai_turn_insert_table_inner(turn, &p[0], 2, 2, true)
            .expect("table");
        let cells: serde_json::Value = d.ai_turn_table_cells_value(turn).expect("cells");
        for (r, row) in ["Name|Age", "Ada|36"].iter().enumerate() {
            for (c, text) in row.split('|').enumerate() {
                let cell = cells["cells"][r][c].as_str().expect("cell paragraph");
                d.ai_turn_set_anchor_inner(turn, cell, 0).expect("anchor");
                d.ai_turn_write_inner(turn, text).expect("write");
            }
        }
        assert_eq!(texts(&d), ["Intro", "[Name|Age|Ada|36]", "After"]);
        let listed = items(&d);
        assert_eq!(listed.len(), 1, "one card for the whole table: {listed:?}");
        assert_eq!(listed[0]["kind"], "table_insertion");
        assert_eq!(listed[0]["author"], "Casual AI");
        assert_eq!(listed[0]["text"], "Name | Age");
        let id = listed[0]["id"].as_str().expect("id").to_owned();
        assert!(id.ends_with(":table"));
        d.ai_turn_end_value(turn).expect("end");
        if accept {
            d.ai_turn_accept_inner(turn).expect("accept all");
            assert!(items(&d).is_empty());
            assert_eq!(texts(&d), ["Intro", "[Name|Age|Ada|36]", "After"]);
            let BlockNode::Table(table) = &d.document.body()[1] else {
                panic!("a table");
            };
            assert!(
                table
                    .rows
                    .iter()
                    .all(|r| r.properties.row_revision.is_none())
            );
            assert!(table.rows[0].properties.header, "the header row repeats");
        } else {
            if d.decide_revision(&id, false).is_err() {
                panic!("reject the table");
            }
            assert_eq!(texts(&d), ["Intro", "After"], "rejecting removes the table");
        }
    }
}

#[test]
fn a_tracked_table_survives_a_docx_round_trip() {
    let mut d = open(&["Intro"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_insert_table_inner(turn, &p[0], 1, 2, false)
        .expect("table");
    let cells: serde_json::Value = d.ai_turn_table_cells_value(turn).expect("cells");
    d.ai_turn_set_anchor_inner(turn, cells["cells"][0][0].as_str().unwrap(), 0)
        .expect("anchor");
    d.ai_turn_write_inner(turn, "x").expect("write");
    let bytes = d
        .export_as_inner(casual_doc_io::formats::DOCX, "preserve_when_safe")
        .expect("save")
        .bytes;
    let reopened = open_document(&bytes).expect("reopen");
    let listed = items(&reopened);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["kind"], "table_insertion");
    assert_eq!(texts(&reopened)[1], "[x|]");
    let BlockNode::Table(table) = &reopened.document.body()[1] else {
        panic!("a table");
    };
    let BlockNode::Paragraph(cell) = &table.rows[0].cells[0].blocks[0] else {
        panic!("a paragraph");
    };
    assert!(
        matches!(&cell.inlines[0], InlineNode::Revision(r) if r.kind == RevisionKind::Insertion),
        "the cell text is still a tracked insertion"
    );
}

#[test]
fn a_direct_table_is_plain() {
    let mut d = open(&["Intro"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("direct", DATE).expect("begin");
    d.ai_turn_insert_table_inner(turn, &p[0], 2, 3, false)
        .expect("table");
    let cells: serde_json::Value = d.ai_turn_table_cells_value(turn).expect("cells");
    d.ai_turn_set_anchor_inner(turn, cells["cells"][1][2].as_str().unwrap(), 0)
        .expect("anchor");
    d.ai_turn_write_inner(turn, "z").expect("write");
    assert_eq!(texts(&d), ["Intro", "[|||||z]"]);
    assert!(items(&d).is_empty(), "no revision at all");
    undo(&mut d);
    assert_eq!(texts(&d), ["Intro"], "table and text are one step");
}

#[test]
fn a_suggested_table_deletion_is_one_card() {
    let mut d = open(&["Intro"]);
    let p = ids(&d);
    // A plain table first, by the person.
    if d.insert_table(&p[0], 1, 2).is_err() {
        panic!("insert a table");
    }
    let table = match &d.document.body()[1] {
        BlockNode::Table(table) => table.id.to_string(),
        _ => panic!("a table"),
    };
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_remove_table_inner(turn, &table).expect("remove");
    let listed = items(&d);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["kind"], "table_deletion");
    d.ai_turn_accept_inner(turn).expect("accept all");
    assert_eq!(texts(&d), ["Intro"], "accepting the deletion removes it");
}

#[test]
fn a_paragraph_after_a_block_and_a_comment_join_the_turn() {
    let mut d = open(&["Alpha", "Beta"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_paragraph_after_inner(turn, &p[0])
        .expect("paragraph");
    d.ai_turn_write_inner(turn, "Middle").expect("write");
    d.ai_turn_comment_inner(turn, &p[1], 0, 4, "Check this")
        .expect("comment");
    assert_eq!(texts(&d), ["Alpha", "Middle", "Beta"]);
    assert_eq!(d.document.definitions().comments.iter().count(), 1);
    d.ai_turn_end_value(turn).expect("end");
    undo(&mut d);
    assert_eq!(texts(&d), ["Alpha", "Beta"]);
    assert_eq!(d.document.definitions().comments.iter().count(), 0);
}

#[test]
fn a_suggested_paragraph_break_splits_and_a_cross_paragraph_delete_is_tracked() {
    let mut d = open(&["Alpha", "Beta", "Gamma"]);
    let p = ids(&d);
    let turn = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(turn, &p[2], 5).expect("anchor");
    d.ai_turn_write_inner(turn, " one\ntwo").expect("write");
    assert_eq!(texts(&d), ["Alpha", "Beta", "Gamma one", "two"]);
    d.ai_turn_delete_range_inner(turn, &p[0], 2, &p[1], 2)
        .expect("delete");
    assert!(
        items(&d).iter().any(|i| i["kind"] == "deletion"),
        "the deletion is a suggestion"
    );
    d.ai_turn_revert_inner(turn, false).expect("reject all");
    assert_eq!(texts(&d), ["Alpha", "Beta", "Gamma"]);
    assert!(items(&d).is_empty());
}

#[test]
fn a_protected_document_refuses_the_turn_as_it_refuses_the_person() {
    let mut d = open(&["Alpha"]);
    let p = ids(&d);
    d.apply(Operation::SetDocumentProtection {
        protection: Some(casual_doc_model::v1::DocumentProtection {
            edit: casual_doc_model::v1::DocumentProtectionEdit::ReadOnly,
            enforcement: true,
            formatting: false,
            password: None,
        }),
    })
    .expect("protect");
    let turn = d.ai_turn_begin_inner("direct", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(turn, &p[0], 0).expect("anchor");
    assert!(
        d.ai_turn_write_inner(turn, "x").is_err(),
        "refused at the choke point"
    );
    assert_eq!(texts(&d), ["Alpha"]);
    let _ = Pos::new(d.document.id(), 0);
}

/// RM-52: two turns started in the SAME second share a `w:date`. The second turn's Accept all
/// must accept its own suggestion only; the first turn's stays pending. (Matching a turn's
/// revisions by author and date, as before, accepted both.)
#[test]
fn turns_started_in_the_same_second_never_decide_each_others_suggestions() {
    let mut d = open(&["Alpha", "Beta"]);
    let p = ids(&d);
    let first = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(first, &p[0], 5).expect("anchor");
    d.ai_turn_write_inner(first, " ONE").expect("write");
    d.ai_turn_end_value(first).expect("end");
    // The person leaves the first turn's card undecided and asks again, in the same second.
    let second = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    assert_ne!(first, second);
    d.ai_turn_set_anchor_inner(second, &p[1], 4)
        .expect("anchor");
    d.ai_turn_write_inner(second, " TWO").expect("write");
    let end = d.ai_turn_end_value(second).expect("end");
    assert_eq!(
        end["revisions"].as_array().map(Vec::len),
        Some(1),
        "the second turn owns one suggestion, not both: {end}"
    );
    d.ai_turn_accept_inner(second).expect("accept all");
    let left = items(&d);
    assert_eq!(
        left.len(),
        1,
        "the first turn's suggestion is still pending: {left:?}"
    );
    assert_eq!(left[0]["text"], " ONE");
    assert_eq!(texts(&d), ["Alpha ONE", "Beta TWO"]);
}

/// RM-52: a turn id is never handed out twice in a document's life, so a card left from a
/// decided turn cannot name the next one. (Restarting at 1 once the turn was decided did.)
#[test]
fn a_decided_turns_id_never_names_a_later_turn() {
    let mut d = open(&["Alpha"]);
    let p = ids(&d);
    let first = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    d.ai_turn_set_anchor_inner(first, &p[0], 5).expect("anchor");
    d.ai_turn_write_inner(first, " one").expect("write");
    d.ai_turn_end_value(first).expect("end");
    d.ai_turn_accept_inner(first).expect("accept");
    let second = d.ai_turn_begin_inner("suggest", DATE).expect("begin");
    assert_ne!(first, second, "a fresh id");
    d.ai_turn_set_anchor_inner(second, &p[0], 0)
        .expect("anchor");
    d.ai_turn_write_inner(second, "two ").expect("write");
    assert!(
        d.ai_turn_accept_inner(first).is_err(),
        "the earlier turn's handle is gone, not the new turn's"
    );
    assert_eq!(items(&d).len(), 1, "the new turn's suggestion is untouched");
}
