// SPDX-License-Identifier: Apache-2.0

//! Host identity in the editor facade — `docs/175` §5.2, §6, ADR-070.
//!
//! The DOCX guards read the saved package's own XML and then open it again, so what is asserted
//! is what Word would read, not what the model holds. The mutations that drove each guard red
//! are recorded in the commit message.

//!
//! The room half of `docs/175` (the relay's stamp, the roster's profiles) is guarded in
//! `collab_tests.rs`, the one test file allowed to name the protocol
//! (`the_live_editor_has_no_collaboration_dependency`).

use super::*;

fn document() -> WasmDocument {
    open_document(b"Shared text for review.").expect("a document opens from plain text")
}

fn first_paragraph(d: &WasmDocument) -> String {
    d.document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
            _ => None,
        })
        .expect("a paragraph")
}

fn part(bytes: &[u8], name: &str) -> Option<String> {
    let mut package = DocxPackage::open(bytes, viewer_limits()).expect("open package");
    package
        .read_part(name)
        .ok()
        .map(|bytes| String::from_utf8(bytes).expect("utf-8"))
}

/// Ada comments and suggests, the way the host's identity names her.
fn authored() -> WasmDocument {
    let mut d = document();
    d.set_active_author(
        "Ada Lovelace",
        Some("AL".to_owned()),
        Some("u_ada".to_owned()),
    )
    .expect("an active author");
    let node = first_paragraph(&d);
    d.add_comment(&node, 0, &node, 6, "Check this.", None, None, None)
        .expect("a comment");
    d.suggest_insert(&node, 0, "Draft: ", None, None, None)
        .expect("a suggestion");
    d
}

/// **The DOCX authorship round-trips**, written as Word writes it: `w:author` and `w:initials`
/// on the comment, `w:author` on the tracked insertion, and — with no opt-in — no
/// `people.xml` and no id of any kind (`docs/175` §6).
#[test]
fn docx_authorship_round_trips_as_word_writes_it() {
    let d = authored();
    let saved = d.export_docx().expect("saves");
    let comments = part(&saved, "word/comments.xml").expect("comments.xml");
    assert!(
        comments.contains(r#"w:author="Ada Lovelace""#) && comments.contains(r#"w:initials="AL""#),
        "{comments}"
    );
    let body = part(&saved, "word/document.xml").expect("document.xml");
    assert!(
        body.contains(r#"w:author="Ada Lovelace""#),
        "the tracked change names Ada"
    );
    assert!(
        part(&saved, "word/people.xml").is_none(),
        "without the host's opt-in no people.xml is written"
    );
    assert!(
        !body.contains("u_ada") && !comments.contains("u_ada"),
        "the wire id never enters the file"
    );

    let reopened = open_document(&saved).expect("reopens");
    let comment = reopened
        .document
        .definitions()
        .comments
        .iter()
        .map(|(_, comment)| comment)
        .next()
        .expect("the comment");
    assert_eq!(comment.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(comment.initials.as_deref(), Some("AL"));
    assert!(
        build_review_revision_summaries(&reopened.document)
            .iter()
            .any(|revision| revision.author.as_deref() == Some("Ada Lovelace")),
        "the suggestion comes back under Ada's name"
    );
    assert!(reopened.document.definitions().people.is_empty());
}

/// With `persist`, the saved file links the author to the host's account the way Word does:
/// `w15:person w15:author` with `w15:presenceInfo w15:providerId w15:userId`, verbatim.
#[test]
fn people_xml_is_written_only_with_the_hosts_persist() {
    let mut d = authored();
    d.set_author_persistence_inner(
        Some("AD".to_owned()),
        Some("S::ada@example.org::a1b2".to_owned()),
    )
    .expect("opted in");
    let saved = d.export_docx().expect("saves");
    let people = part(&saved, "word/people.xml").expect("people.xml with persist");
    assert!(people.contains(r#"w15:author="Ada Lovelace""#), "{people}");
    assert!(
        people.contains(r#"w15:providerId="AD""#)
            && people.contains(r#"w15:userId="S::ada@example.org::a1b2""#),
        "{people}"
    );
    let reopened = open_document(&saved).expect("reopens");
    let person = &reopened.document.definitions().people[0];
    assert_eq!(person.author, "Ada Lovelace");
    assert_eq!(
        person.presence.as_ref().map(|p| p.user_id.as_str()),
        Some("S::ada@example.org::a1b2")
    );
    // And it is saved again unchanged, not duplicated.
    let mut again = reopened;
    again
        .set_active_author("Ada Lovelace", None, None)
        .expect("same author");
    again
        .set_author_persistence_inner(
            Some("AD".to_owned()),
            Some("S::ada@example.org::a1b2".to_owned()),
        )
        .expect("opted in");
    let resaved = again.export_docx().expect("saves again");
    let people = part(&resaved, "word/people.xml").expect("people.xml");
    assert_eq!(people.matches("<w15:person ").count(), 1, "{people}");

    // Opting out writes no id at all.
    d.set_author_persistence_inner(None, None)
        .expect("opted out");
    assert!(part(&d.export_docx().expect("saves"), "word/people.xml").is_none());
}

/// Two people with one display name are one person once saved (`people.xml` joins by name),
/// so the editor refuses to attribute one's work to the other's account and says so.
#[test]
fn a_name_already_in_people_xml_under_another_account_is_refused_not_merged() {
    let mut d = authored();
    d.document
        .definitions_mut()
        .people
        .push(casual_doc_model::v1::Person {
            author: "Ada Lovelace".to_owned(),
            presence: Some(casual_doc_model::v1::PresenceInfo {
                provider_id: "AD".to_owned(),
                user_id: "somebody-else".to_owned(),
            }),
        });
    d.set_author_persistence_inner(Some("AD".to_owned()), Some("u-ada".to_owned()))
        .expect("opted in");
    let refused = d
        .with_persisted_author()
        .expect_err("a collision is refused");
    assert!(refused.contains("identity-name-collision"), "{refused}");
}
