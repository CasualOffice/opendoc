// SPDX-License-Identifier: Apache-2.0

//! The live editor's entry point (`docs/181` §4.4): a plan compiled against
//! the editor's document returns its redline and its operations and changes
//! nothing; the ids it mints are past the editor's allocator; it is the same
//! run `preview` and `apply` are.

use casual_doc_ai::editor;
use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read;
use casual_doc_ai::{Clock, Profile, Session, SessionConfig};
use casual_doc_model::{IdGenerator, IdSpace};
use casual_doc_review::body::RevisionIdAllocator;
use serde_json::{Value, json};

const AUTHOR: &str = "Ada Lovelace (AI)";
/// The editor's document revision the session is built at. Not zero, so a
/// test can tell the editor's revision from the session log's own.
const EDITOR_REVISION: u64 = 41;

fn config(profile: Profile) -> SessionConfig {
    SessionConfig {
        author: AUTHOR.to_owned(),
        profile,
        clock: Clock::Fixed("2026-10-28T09:00:00Z".to_owned()),
        ..SessionConfig::default()
    }
}

/// The editor's id allocator as the wasm editor holds it: the document's
/// LOCAL space, already advanced past ids the editor minted for content an
/// undo has since removed (so the document's own high-water mark is lower).
fn editor_ids(session: &Session) -> IdGenerator {
    let space = IdSpace::local(IdSpace::of_document(session.document().id()));
    let mut ids = IdGenerator::new(space.get());
    ids.reserve_through(session.document().highest_counter_in(space) + 5_000);
    ids
}

/// A session over "the editor's" document, built the way the wasm binding
/// builds it.
fn live(profile: Profile) -> Session {
    let text = "The notice period is thirty (30) days.\nTermination for convenience applies.\n";
    let file = Session::open(text.as_bytes(), "contract.txt", config(profile)).expect("opens");
    let ids = editor_ids(&file);
    Session::for_editor(
        file.document().clone(),
        ids,
        RevisionIdAllocator::from_document(file.document()),
        EDITOR_REVISION,
        config(profile),
    )
}

fn node_of(session: &Session, needle: &str) -> String {
    let hits = read::search(session, needle, read::SearchMode::Exact, None, None).expect("search");
    hits["hits"][0]["node"].as_str().expect("a hit").to_owned()
}

fn plan(value: Value) -> EditPlan {
    serde_json::from_value(value).expect("a well-formed plan")
}

fn replace_and_add(session: &Session) -> EditPlan {
    let node = node_of(session, "thirty");
    plan(json!({
        "base_revision": session.revision(),
        "edits": [
            { "op": "replace_text", "target": { "node": node, "exact": "thirty (30)" }, "text": "sixty (60)" },
            { "op": "insert_paragraphs", "after": { "node": node }, "paragraphs": [{ "text": "Notice is given in writing." }] },
        ],
    }))
}

#[test]
fn a_session_over_the_editor_reports_the_editors_revision() {
    let session = live(Profile::Suggest);
    assert_eq!(session.revision(), EDITOR_REVISION);
    let outline = read::outline(&session, None, None, None).expect("outline");
    assert_eq!(outline["revision"], EDITOR_REVISION);
}

#[test]
fn compiling_returns_the_redline_and_the_operations_and_changes_nothing() {
    let session = live(Profile::Suggest);
    let before = session.digest();
    let compiled = editor::compile(&session, &replace_and_add(&session)).expect("compiles");

    assert!(compiled.would_apply, "{}", compiled.preview);
    assert!(!compiled.operations.is_empty());
    assert_eq!(compiled.preview["author"], AUTHOR);
    assert_eq!(compiled.preview["mode"], "suggest");
    let redline = compiled.preview["redline"].as_array().expect("a redline");
    // The edited paragraph, with the deletion and the insertion marked …
    let edited = redline
        .iter()
        .find(|block| {
            block["before"]
                .as_str()
                .is_some_and(|text| text.contains("thirty"))
        })
        .expect("the edited paragraph is in the redline");
    let marks: Vec<&str> = edited["afterRuns"]
        .as_array()
        .expect("runs")
        .iter()
        .flat_map(|run| run["marks"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    assert!(
        marks.contains(&"deleted") && marks.contains(&"inserted"),
        "{edited}"
    );
    // … and the new one, which had no `before`.
    assert!(
        redline.iter().any(|block| block["before"].is_null()
            && block["after"].as_str() == Some("Notice is given in writing.")),
        "{redline:?}"
    );

    // Nothing was committed: same content, same revision, no change recorded.
    assert_eq!(session.digest(), before);
    assert_eq!(session.revision(), EDITOR_REVISION);
}

#[test]
fn the_compile_is_the_preview_and_is_deterministic() {
    let session = live(Profile::Suggest);
    let plan = replace_and_add(&session);
    let compiled = editor::compile(&session, &plan).expect("compiles");
    let previewed = plan::preview(&session, &plan).expect("previews");
    assert_eq!(compiled.preview, previewed, "one run, one report");

    let compiled_again = editor::compile(&live(Profile::Suggest), &plan).expect("compiles");
    assert_eq!(
        format!("{:?}", compiled.operations),
        format!("{:?}", compiled_again.operations),
        "a compile is deterministic for one document and one allocator"
    );
}

#[test]
fn every_id_a_plan_mints_is_below_the_allocator_it_hands_back() {
    let session = live(Profile::Suggest);
    let space = IdSpace::local(IdSpace::of_document(session.document().id()));
    let start = editor_ids(&session).next_counter();
    let plan = replace_and_add(&session);
    let compiled = editor::compile(&session, &plan).expect("compiles");

    // What the plan put in the document, from the same run applied.
    let mut applied = live(Profile::Suggest);
    plan::apply(&mut applied, &plan).expect("applies");
    let highest = applied.document().highest_counter_in(space);

    assert!(
        highest >= start,
        "the plan minted in the editor's space, from the editor's counter ({highest} < {start})"
    );
    assert_eq!(compiled.ids.namespace(), space.get());
    assert!(
        compiled.ids.next_counter() > highest,
        "the editor moves its allocator to {} and the plan minted {highest}: the next id the \
         editor mints would collide",
        compiled.ids.next_counter()
    );
}

#[test]
fn a_plan_the_profile_does_not_allow_compiles_to_refusals_and_no_operations() {
    // `readonly` maps to the read profile: nothing an edit plan does is admitted.
    let session = live(Profile::Read);
    let compiled = editor::compile(&session, &replace_and_add(&session)).expect("compiles");
    assert!(!compiled.would_apply);
    assert!(compiled.operations.is_empty());
    assert_eq!(compiled.preview["refusals"], 2);
    assert_eq!(
        compiled.preview["edits"][0]["refusal"]["code"],
        "capability-withheld"
    );

    // A suggester asking for a direct edit is refused too.
    let session = live(Profile::Suggest);
    let node = node_of(&session, "thirty");
    let direct = plan(json!({
        "base_revision": session.revision(),
        "mode": "direct",
        "edits": [{ "op": "insert_text", "at": { "node": node, "exact": "days" }, "text": " net" }],
    }));
    let compiled = editor::compile(&session, &direct).expect("compiles");
    assert!(!compiled.would_apply);
    assert!(compiled.operations.is_empty());
}

#[test]
fn a_plan_written_against_another_revision_is_stale() {
    let session = live(Profile::Suggest);
    let node = node_of(&session, "thirty");
    let stale = plan(json!({
        "base_revision": EDITOR_REVISION - 1,
        "edits": [{ "op": "insert_text", "at": { "node": node, "exact": "days" }, "text": " net" }],
    }));
    let refused = editor::compile(&session, &stale).expect_err("stale");
    assert_eq!(refused.code, "stale-revision");
}
