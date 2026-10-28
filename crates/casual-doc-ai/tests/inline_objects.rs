// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §4.2 steps 4-6 (lane families-b): what an agent's links,
//! pictures, headers and page setup refuse, and what they keep.
//!
//! The happy path of every command is `suggest_roundtrip.rs`'s direct-example
//! guard; this file holds the bounds an untrusted plan meets.

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat};
use casual_doc_ai::{Clock, Profile, Session, SessionConfig};
use casual_doc_model::v1::{BlockNode, InlineNode};
use serde_json::{Value, json};

const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";

fn session() -> (Session, String) {
    let config = SessionConfig {
        author: "Claude (AI)".to_owned(),
        profile: Profile::Edit,
        clock: Clock::Fixed("2026-10-28T00:00:00Z".to_owned()),
        ..SessionConfig::default()
    };
    let session = Session::open(
        b"The notice period is thirty (30) days.\n",
        "contract.txt",
        config,
    )
    .expect("opens");
    let blocks =
        read::read_blocks(&session, None, None, Some(20_000), ReadFormat::Json).expect("reads");
    let node = blocks["blocks"][0]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    (session, node)
}

fn apply(session: &mut Session, edit: Value) -> Result<Value, casual_doc_ai::AiError> {
    let plan: EditPlan = serde_json::from_value(json!({
        "base_revision": session.revision(),
        "mode": "direct",
        "edits": [edit],
    }))
    .expect("a well-formed plan");
    plan::apply(session, &plan)
}

fn refusal(error: &casual_doc_ai::AiError) -> String {
    error.details["refusals"][0]["refusal"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn an_agent_link_is_a_web_mail_or_bookmark_target_never_a_script() {
    let (mut session, node) = session();
    for target in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html,x",
        "#",
    ] {
        let refused = apply(
            &mut session,
            json!({ "op": "set_hyperlink", "target": { "node": node, "exact": "notice" },
                    "url": target }),
        )
        .expect_err("refused");
        assert!(
            refusal(&refused).contains("link target") || refusal(&refused).contains('#'),
            "{target}: {refused:?}"
        );
    }
    apply(
        &mut session,
        json!({ "op": "set_hyperlink", "target": { "node": node, "exact": "notice" },
                "url": "https://example.com/terms", "tooltip": "Terms" }),
    )
    .expect("a web link applies");
}

#[test]
fn a_picture_is_the_format_it_claims_and_keeps_its_proportions() {
    let (mut session, node) = session();
    let refused = apply(
        &mut session,
        json!({ "op": "insert_image", "at": { "node": node }, "data": PNG_1X1,
                "content_type": "image/jpeg" }),
    )
    .expect_err("a PNG labelled JPEG is refused");
    assert!(
        refusal(&refused).contains("not a image/jpeg"),
        "{refused:?}"
    );
    let refused = apply(
        &mut session,
        json!({ "op": "insert_image", "at": { "node": node }, "data": "not base64!",
                "content_type": "image/png" }),
    )
    .expect_err("not base64");
    assert!(refusal(&refused).contains("base64"), "{refused:?}");

    apply(
        &mut session,
        json!({ "op": "insert_image", "at": { "node": node }, "data": PNG_1X1,
                "content_type": "image/png", "width_pt": 36, "alt_text": "A dot" }),
    )
    .expect("a picture inserts");
    let drawing = session
        .document()
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.inlines.iter().find_map(|inline| match inline {
                    InlineNode::Drawing(drawing) => Some(drawing.as_ref().clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .expect("the picture is in the paragraph");
    let extent = drawing.extent.expect("an extent");
    // 36 pt wide, and the 1x1 picture's own proportions make it 36 pt tall.
    assert_eq!((extent.width_emu, extent.height_emu), (457_200, 457_200));
    assert_eq!(drawing.descr.as_deref(), Some("A dot"));
}

#[test]
fn a_bookmark_name_follows_words_rule_and_is_unique() {
    let (mut session, node) = session();
    for name in ["1st", "has space", "x-y", ""] {
        apply(
            &mut session,
            json!({ "op": "create_bookmark", "target": { "node": node, "exact": "notice" },
                    "name": name }),
        )
        .expect_err(name);
    }
    apply(
        &mut session,
        json!({ "op": "create_bookmark", "target": { "node": node, "exact": "notice" },
                "name": "Notice_1" }),
    )
    .expect("a Word-valid name applies");
    let refused = apply(
        &mut session,
        json!({ "op": "create_bookmark", "target": { "node": node, "exact": "period" },
                "name": "Notice_1" }),
    )
    .expect_err("a second bookmark of the same name");
    assert!(refusal(&refused).contains("exists"), "{refused:?}");
    apply(
        &mut session,
        json!({ "op": "rename_bookmark", "name": "Notice_1", "new_name": "Clause9" }),
    )
    .expect("renames");
    apply(
        &mut session,
        json!({ "op": "delete_bookmark", "name": "Clause9" }),
    )
    .expect("deletes");
    let refused = apply(
        &mut session,
        json!({ "op": "delete_bookmark", "name": "Clause9" }),
    )
    .expect_err("gone");
    assert!(refusal(&refused).contains("No bookmark"), "{refused:?}");
}

#[test]
fn a_header_is_added_once_and_its_paragraph_is_reported() {
    // A LibreOffice-written DOCX with one section and no header.
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/corpus/real-producer-footnotes.docx"
    ))
    .expect("the committed corpus fixture");
    let config = SessionConfig {
        profile: Profile::Edit,
        ..SessionConfig::default()
    };
    let mut session = Session::open(&bytes, "footnotes.docx", config).expect("opens");
    assert!(session.document().definitions().headers.is_empty());
    let applied = apply(&mut session, json!({ "op": "insert_header" })).expect("adds one");
    assert_eq!(session.document().definitions().headers.len(), 1);
    assert_eq!(
        applied["edits"][0]["nodes"].as_array().map(Vec::len),
        Some(1)
    );
    let refused = apply(&mut session, json!({ "op": "insert_header" }))
        .expect_err("the section already shows one");
    assert!(
        refusal(&refused).contains("already shows one"),
        "{refused:?}"
    );
}
