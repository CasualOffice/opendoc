// SPDX-License-Identifier: Apache-2.0

//! The M2 guards of `docs/178` §8: an agent's edit lands as a Word tracked
//! change by the configured author; a direct edit is refused without a grant;
//! preview equals apply; anchors refuse ambiguity; a change can be undone.

use std::io::Read;

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read;
use casual_doc_ai::{Clock, ExportFormat, Profile, Session, SessionConfig};
use serde_json::{Value, json};

const AUTHOR: &str = "Claude (AI)";

fn config(profile: Profile) -> SessionConfig {
    SessionConfig {
        author: AUTHOR.to_owned(),
        profile,
        clock: Clock::Fixed("2026-10-24T00:00:00Z".to_owned()),
        ..SessionConfig::default()
    }
}

fn open(profile: Profile) -> Session {
    let text = "The notice period is thirty (30) days.\nTermination for convenience applies.\n";
    Session::open(text.as_bytes(), "contract.txt", config(profile)).expect("a text file opens")
}

/// The id of the paragraph holding `needle`, from the agent's own search tool.
fn node_of(session: &Session, needle: &str) -> String {
    let hits = read::search(session, needle, read::SearchMode::Exact, None, None).expect("search");
    hits["hits"][0]["node"].as_str().expect("a hit").to_owned()
}

fn plan(value: Value) -> EditPlan {
    serde_json::from_value(value).expect("a well-formed plan")
}

fn document_xml(docx: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx)).expect("a zip");
    let mut part = archive.by_name("word/document.xml").expect("the main part");
    let mut xml = String::new();
    part.read_to_string(&mut xml).expect("utf-8");
    xml
}

#[test]
fn a_suggested_replacement_is_saved_as_word_tracked_changes_by_the_configured_author() {
    let mut session = open(Profile::Suggest);
    let node = node_of(&session, "thirty");
    let rev = session.revision();
    let applied = plan::apply(
        &mut session,
        &plan(json!({
            "base_revision": rev,
            "edits": [{
                "op": "replace_text",
                "target": { "node": node, "exact": "thirty (30) days" },
                "text": "sixty (60) days",
            }],
        })),
    )
    .expect("the suggestion applies");
    assert!(applied["changeId"].as_str().is_some());

    let docx = session
        .export(ExportFormat::Docx, &[])
        .expect("a DOCX save of a text-born document loses nothing");
    let xml = document_xml(&docx.bytes);
    assert!(
        xml.contains("<w:ins "),
        "a tracked insertion is written: {xml}"
    );
    assert!(xml.contains("<w:del "), "a tracked deletion is written");
    assert!(
        xml.contains("<w:delText"),
        "the deleted text survives as w:delText"
    );
    assert!(
        xml.contains(&format!("w:author=\"{AUTHOR}\"")),
        "authored by the session's identity, never the plan's"
    );
    assert!(xml.contains("sixty (60) days") && xml.contains("thirty (30) days"));

    // And it reopens as two tracked changes a reviewer can decide.
    let reopened = Session::open(&docx.bytes, "contract.docx", config(Profile::Review))
        .expect("the saved DOCX reopens");
    let changes = read::list_tracked_changes(&reopened, None, None).expect("list");
    let kinds: Vec<&str> = changes["trackedChanges"]
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(|change| change["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"insertion") && kinds.contains(&"deletion"),
        "{changes}"
    );
}

#[test]
fn a_direct_edit_is_refused_without_an_edit_grant_and_allowed_with_one() {
    let mut session = open(Profile::Suggest);
    let node = node_of(&session, "thirty");
    let direct = json!({
        "base_revision": session.revision(),
        "mode": "direct",
        "edits": [{ "op": "insert_text", "at": { "node": node, "exact": "days" }, "text": " net" }],
    });
    let before = session.digest();
    let refused = plan::apply(&mut session, &plan(direct.clone())).expect_err("no grant");
    assert_eq!(refused.code, "plan-refused");
    assert_eq!(refused.details["firstCode"], "capability-withheld");
    assert_eq!(session.digest(), before, "a refused plan changes nothing");

    let mut granted = open(Profile::Edit);
    let node = node_of(&granted, "thirty");
    let mut direct = direct;
    direct["base_revision"] = json!(granted.revision());
    direct["edits"][0]["at"]["node"] = json!(node);
    plan::apply(&mut granted, &plan(direct)).expect("the grant admits a direct edit");
    let changes = read::list_tracked_changes(&granted, None, None).expect("list");
    assert_eq!(changes["total"], 0, "a direct edit is not a tracked change");
}

#[test]
fn preview_reports_exactly_what_apply_does_and_commits_nothing() {
    let mut session = open(Profile::Suggest);
    let node = node_of(&session, "Termination");
    let request = plan(json!({
        "base_revision": session.revision(),
        "edits": [
            { "op": "insert_text", "at": { "node": node, "exact": "applies" }, "text": " in full" },
            { "op": "comment", "target": { "node": node, "exact": "Termination" }, "text": "Aligned with the MSA." },
        ],
    }));
    let revision = session.revision();
    let preview = plan::preview(&session, &request).expect("preview");
    assert_eq!(session.revision(), revision, "preview commits nothing");
    assert_eq!(preview["wouldApply"], true);
    let applied = plan::apply(&mut session, &request).expect("apply");
    assert_eq!(preview["redline"], applied["redline"]);
    assert_eq!(preview["operations"], applied["operations"]);
}

#[test]
fn an_ambiguous_quote_is_refused_with_candidates_rather_than_edited() {
    let text = "Pay the fee. Then pay the fee again.\n";
    let mut session =
        Session::open(text.as_bytes(), "fees.txt", config(Profile::Suggest)).expect("open");
    let node = node_of(&session, "fee");
    let request = |anchor: Value| {
        plan(json!({
            "base_revision": 0,
            "edits": [{ "op": "delete_text", "target": anchor }],
        }))
    };
    let refused = plan::apply(
        &mut session,
        &request(json!({ "node": node, "exact": "the fee" })),
    )
    .expect_err("two matches");
    assert_eq!(
        refused.details["refusals"][0]["refusal"]["code"],
        "anchor-ambiguous"
    );
    plan::apply(
        &mut session,
        &request(json!({ "node": node, "exact": "the fee", "suffix": " again" })),
    )
    .expect("a suffix disambiguates");
}

#[test]
fn the_latest_change_undoes_and_an_older_one_is_refused() {
    let mut session = open(Profile::Suggest);
    let node = node_of(&session, "thirty");
    let original = session.digest();
    let rev = session.revision();
    let first = plan::apply(
        &mut session,
        &plan(json!({
            "base_revision": rev,
            "edits": [{ "op": "delete_text", "target": { "node": node, "exact": "(30) " } }],
        })),
    )
    .expect("first");
    let rev = session.revision();
    let second = plan::apply(
        &mut session,
        &plan(json!({
            "base_revision": rev,
            "edits": [{ "op": "insert_text", "at": { "node": node, "side": "before" }, "text": "Note: " }],
        })),
    )
    .expect("second");
    let older = session
        .undo_change(first["changeId"].as_str().expect("id"))
        .expect_err("not the latest");
    assert_eq!(older.code, "undo-not-latest");
    session
        .undo_change(second["changeId"].as_str().expect("id"))
        .expect("latest");
    session
        .undo_change(first["changeId"].as_str().expect("id"))
        .expect("now latest");
    assert_eq!(
        session.digest(),
        original,
        "both undone restores the document"
    );
}

#[test]
fn a_stale_plan_and_an_unknown_edit_are_refused() {
    let mut session = open(Profile::Suggest);
    let stale = plan::apply(
        &mut session,
        &plan(json!({ "base_revision": 7, "edits": [{ "op": "delete_text", "target": { "node": "x" } }] })),
    )
    .expect_err("stale");
    assert_eq!(stale.code, "stale-revision");
    let unknown = plan::apply(
        &mut session,
        &plan(json!({ "base_revision": 0, "edits": [{ "op": "set_xml", "xml": "<w:p/>" }] })),
    )
    .expect_err("closed vocabulary");
    assert_eq!(unknown.code, "unknown-edit");
}
