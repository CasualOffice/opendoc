// SPDX-License-Identifier: Apache-2.0

//! Authoring content controls, their locks, forms protection over them and the
//! form-roles sidecar (`docs/109` RM-34, `docs/153` `page.contentControls`,
//! `review.protect-form`, `shell.form-roles`), and the checker's contrast rule
//! (HF-064).

use casual_doc_edit::{Operation, Pos, find_paragraph_any};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Color, InlineNode, RgbColor, Run, RunProperties};
use serde_json::Value;

use crate::{WasmDocument, open_document, write_document};

const FORMS: &[u8] = include_bytes!("../../../fixtures/generated/forms-protected.docx");

/// The fixture, unprotected, so a control can be authored in it.
fn authoring() -> WasmDocument {
    let mut d = open_document(FORMS).expect("open the forms fixture");
    d.set_document_protection_inner(None, false, false)
        .expect("lift the protection to author");
    d
}

fn paragraph_with(d: &WasmDocument, needle: &str) -> NodeId {
    d.ordered_paragraphs()
        .into_iter()
        .map(|(id, _)| id)
        .find(|id| {
            find_paragraph_any(&d.document, *id)
                .is_some_and(|paragraph| node_plain_text(&paragraph.inlines).contains(needle))
        })
        .unwrap_or_else(|| panic!("the fixture has a paragraph with {needle:?}"))
}

fn text_of(d: &WasmDocument, paragraph: NodeId) -> String {
    node_plain_text(&find_paragraph_any(&d.document, paragraph).unwrap().inlines)
}

fn controls(d: &WasmDocument) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(&d.content_controls()).unwrap()
}

fn only_control(d: &WasmDocument) -> Value {
    let all = controls(d);
    assert_eq!(all.len(), 1, "one control: {all:?}");
    all.into_iter().next().unwrap()
}

/// "End of form." with a plain-text control holding its placeholder at the end.
fn with_plain_text(d: &mut WasmDocument) -> (NodeId, String) {
    let end = paragraph_with(d, "End of form");
    let len = u32::try_from(text_of(d, end).len()).unwrap();
    d.insert_content_control_inner(&end.to_string(), len, len, "plainText", "Click here.", None)
        .expect("insert a plain-text control");
    let id = only_control(d)["id"].as_str().unwrap().to_owned();
    (end, id)
}

#[test]
fn an_empty_control_holds_its_placeholder_and_undo_takes_it_away() {
    let mut d = authoring();
    let digest = d.content_digest();
    let (end, _) = with_plain_text(&mut d);
    let control = only_control(&d);
    assert_eq!(control["kind"], "plainText");
    assert_eq!(control["showingPlaceholder"], true);
    assert_eq!(control["text"], "Click here.");
    assert!(
        control["controlId"]
            .as_str()
            .is_some_and(|id| id.parse::<u64>().is_ok()),
        "an inserted control gets a numeric w:id: {control}"
    );
    assert_eq!(text_of(&d, end), "End of form.Click here.");
    d.undo_inner().expect("undo the insertion");
    assert!(controls(&d).is_empty());
    assert_eq!(
        d.content_digest(),
        digest,
        "undo restores the document exactly"
    );
}

#[test]
fn a_selection_is_wrapped_and_the_control_survives_a_docx_round_trip() {
    let mut d = authoring();
    let end = paragraph_with(&d, "End of form");
    d.insert_content_control_inner(
        &end.to_string(),
        0,
        3,
        "dropDownList",
        "Choose an item.",
        None,
    )
    .expect("wrap \u{201c}End\u{201d}");
    let control = only_control(&d);
    assert_eq!(control["text"], "End", "the selection is the content");
    assert_eq!(control["showingPlaceholder"], false);
    assert_eq!(text_of(&d, end), "End of form.", "the text is unchanged");
    let id = control["id"].as_str().unwrap();
    d.set_content_control_properties_inner(
        id,
        r#"{"title":"Outcome","tag":"outcome","lockControl":true,"items":[{"display":"Yes","value":"y"},{"display":"No","value":"n"}]}"#,
    )
    .expect("configure it");

    let bytes = write_document(&d.document, d.resources.as_map()).expect("save");
    let reopened = open_document(&bytes).expect("reopen");
    let control = only_control(&reopened);
    assert_eq!(control["title"], "Outcome");
    assert_eq!(control["tag"], "outcome");
    assert_eq!(control["lock"], "sdtLocked");
    assert_eq!(control["kind"], "dropDownList");
    let items: Vec<&str> = control["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["display"].as_str().unwrap())
        .collect();
    assert_eq!(items, ["Yes", "No"]);
}

#[test]
fn a_check_box_is_inserted_with_words_glyphs_and_ticks() {
    let mut d = authoring();
    let end = paragraph_with(&d, "End of form");
    d.insert_content_control_inner(&end.to_string(), 0, 0, "checkbox", "", None)
        .expect("insert a check box");
    let control = only_control(&d);
    assert_eq!(control["kind"], "checkbox");
    assert_eq!(control["checked"], false);
    assert!(text_of(&d, end).starts_with('\u{2610}'));
    let id = control["id"].as_str().unwrap().to_owned();
    d.toggle_form_checkbox_inner(&id).expect("tick it");
    assert_eq!(only_control(&d)["checked"], true);
    assert!(text_of(&d, end).starts_with('\u{2612}'));
}

#[test]
fn filling_replaces_the_placeholder_and_remove_keeps_the_content() {
    let mut d = authoring();
    let (end, id) = with_plain_text(&mut d);
    d.fill_content_control_inner(&id, r#"{"text":"Ada"}"#)
        .expect("fill it");
    let control = only_control(&d);
    assert_eq!(control["showingPlaceholder"], false);
    assert_eq!(text_of(&d, end), "End of form.Ada");

    d.remove_content_control_inner(&id).expect("remove it");
    assert!(controls(&d).is_empty());
    assert_eq!(text_of(&d, end), "End of form.Ada", "the content stays");
}

#[test]
fn changing_an_inline_controls_properties_leaves_the_caret_in_that_control() {
    // Word leaves the caret where it was. It used to land at the paragraph's
    // start, outside a control that does not open the paragraph, so the next
    // Alt+Down, Fill or Properties found no control.
    let mut d = authoring();
    let end = paragraph_with(&d, "End of form");
    d.insert_content_control_inner(
        &end.to_string(),
        4,
        4,
        "dropDownList",
        "Choose an item.",
        None,
    )
    .expect("insert after \u{201c}End \u{201d}");
    let id = only_control(&d)["id"].as_str().unwrap().to_owned();
    let result = d
        .set_content_control_properties_inner(
            &id,
            r#"{"title":"Answer","items":[{"display":"Yes","value":""}]}"#,
        )
        .expect("configure it");
    assert_eq!(result.node(), end.to_string());
    let at = d
        .content_control_at(&result.node(), result.offset())
        .expect("the caret is in a control");
    let at: Value = serde_json::from_str(&at).unwrap();
    assert_eq!(
        at["id"], id,
        "the caret is in the control whose properties changed"
    );
}

#[test]
fn a_content_locked_control_refuses_typing_and_a_control_locked_one_refuses_removal() {
    let mut d = authoring();
    let end = paragraph_with(&d, "End of form");
    d.insert_content_control_inner(&end.to_string(), 0, 3, "richText", "x", None)
        .unwrap();
    let id = only_control(&d)["id"].as_str().unwrap().to_owned();
    d.set_content_control_properties_inner(&id, r#"{"lockContent":true,"lockControl":true}"#)
        .unwrap();
    let typed = d.apply(Operation::InsertText {
        at: Pos::new(end, 1),
        text: "Q".to_owned(),
    });
    let refusal = typed.expect_err("content-locked refuses typing inside");
    assert!(refusal.contains("locked"), "{refusal}");
    // Outside the control the paragraph is still editable.
    d.apply(Operation::InsertText {
        at: Pos::new(end, 5),
        text: "Q".to_owned(),
    })
    .expect("typing outside a locked control is not refused");
    let removed = d.remove_content_control_inner(&id);
    assert!(
        removed
            .expect_err("sdtLocked refuses removal")
            .contains("deleted")
    );
    let deleted = d.apply(Operation::DeleteText {
        range: casual_doc_edit::Range {
            start: Pos::new(end, 0),
            end: Pos::new(end, 4),
        },
    });
    assert!(deleted.is_err(), "deleting across it whole is refused too");
}

#[test]
fn forms_protection_lets_a_reader_fill_a_control_and_nothing_else() {
    let mut d = authoring();
    let (end, id) = with_plain_text(&mut d);
    d.fill_content_control_inner(&id, r#"{"text":"Ada"}"#)
        .unwrap();
    d.set_document_protection_inner(Some("forms"), true, false)
        .expect("protect the form");
    // "End of form." is 12 bytes; the control is 12..15.
    d.apply(Operation::InsertText {
        at: Pos::new(end, 14),
        text: "m".to_owned(),
    })
    .expect("typing inside the control is filling the form");
    assert_eq!(text_of(&d, end), "End of form.Adma");
    // The caret sits at the control's end after each character: that is inside.
    d.apply(Operation::InsertText {
        at: Pos::new(end, 16),
        text: "x".to_owned(),
    })
    .expect("typing at the control's end is still filling it");
    assert_eq!(only_control(&d)["text"], "Admax");
    let outside = d.apply(Operation::InsertText {
        at: Pos::new(end, 2),
        text: "Z".to_owned(),
    });
    assert!(
        outside
            .expect_err("typing outside the control is refused")
            .contains("form fields")
    );
    // A content-locked control is not a field the reader may fill.
    d.set_document_protection_inner(None, false, false).unwrap();
    d.set_content_control_properties_inner(&id, r#"{"lockContent":true}"#)
        .unwrap();
    d.set_document_protection_inner(Some("forms"), true, false)
        .unwrap();
    assert!(
        d.fill_content_control_inner(&id, r#"{"text":"B"}"#)
            .is_err()
    );
}

#[test]
fn a_role_fills_only_its_own_controls() {
    let mut d = authoring();
    let (end, id) = with_plain_text(&mut d);
    d.set_form_roles_inner(
        r##"{"roles":[{"name":"Employee","color":"3355c4"},{"name":"Manager","color":"#aa3300"}]}"##,
    )
    .expect("two roles");
    d.set_content_control_properties_inner(&id, r#"{"role":"Manager"}"#)
        .expect("assign the control to Manager");
    assert_eq!(only_control(&d)["role"], "Manager");
    d.set_document_protection_inner(Some("forms"), true, false)
        .unwrap();

    d.set_form_fill_role_inner(Some("Employee".to_owned()))
        .unwrap();
    assert_eq!(only_control(&d)["fillable"], false);
    let refused = d.fill_content_control_inner(&id, r#"{"text":"Ada"}"#);
    assert!(
        refused
            .expect_err("another role's field")
            .contains("another role")
    );
    let typed = d.apply(Operation::InsertText {
        at: Pos::new(end, 13),
        text: "m".to_owned(),
    });
    assert!(
        typed.is_err(),
        "typing into another role's field is refused at the choke point"
    );

    d.set_form_fill_role_inner(Some("Manager".to_owned()))
        .unwrap();
    d.fill_content_control_inner(&id, r#"{"text":"Ada"}"#)
        .expect("the Manager fills the Manager's field");

    // The sidecar names the control by its w:id, and drops a deleted role's.
    let roles: Value = serde_json::from_str(&d.form_roles_json()).unwrap();
    let key = only_control(&d)["controlId"].as_str().unwrap().to_owned();
    assert_eq!(roles["assignments"][&key], "Manager");
    d.set_form_roles_inner(r#"{"roles":[{"name":"Employee","color":"3355c4"}]}"#)
        .unwrap();
    assert_eq!(only_control(&d)["role"], Value::Null);
    assert!(
        d.set_form_roles_inner(r#"{"roles":[{"name":"A","color":"x"}]}"#)
            .is_err()
    );
}

#[test]
fn a_date_fill_stores_the_iso_date_and_the_shown_text() {
    let mut d = authoring();
    let end = paragraph_with(&d, "End of form");
    d.insert_content_control_inner(&end.to_string(), 0, 0, "date", "Pick a date.", None)
        .unwrap();
    let id = only_control(&d)["id"].as_str().unwrap().to_owned();
    d.set_content_control_properties_inner(&id, r#"{"dateFormat":"d MMMM yyyy"}"#)
        .unwrap();
    d.fill_content_control_inner(
        &id,
        r#"{"text":"5 March 2026","fullDate":"2026-03-05T00:00:00Z"}"#,
    )
    .unwrap();
    let control = only_control(&d);
    assert_eq!(control["dateFormat"], "d MMMM yyyy");
    assert_eq!(control["fullDate"], "2026-03-05T00:00:00Z");
    assert_eq!(control["text"], "5 March 2026");
}

#[test]
fn light_grey_text_on_white_is_reported_and_black_is_not() {
    let mut d = authoring();
    let end = paragraph_with(&d, "End of form");
    let grey = RunProperties {
        color: Some(Color::Rgb(RgbColor {
            r: 0xCC,
            g: 0xCC,
            b: 0xCC,
        })),
        ..RunProperties::default()
    };
    let run_id = d.edit_ids.next_id().unwrap();
    d.apply(Operation::SetInlines {
        node: end,
        inlines: vec![InlineNode::Run(Run {
            id: run_id,
            properties: grey.into(),
            text: "faint".to_owned(),
        })],
    })
    .unwrap();
    let findings = d.contrast_findings();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].node, end.to_string());
    assert_eq!(findings[0].foreground, "CCCCCC");
    assert_eq!(findings[0].background, "FFFFFF");
    assert!(findings[0].ratio < 2.0);
    assert!(
        (crate::accessibility_check::contrast_ratio(
            RgbColor { r: 0, g: 0, b: 0 },
            RgbColor {
                r: 255,
                g: 255,
                b: 255
            }
        ) - 21.0)
            .abs()
            < 1e-9
    );
}
