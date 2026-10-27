// SPDX-License-Identifier: Apache-2.0

//! `docs/181` (ADR-075): the edit vocabulary IS the capability registry, an
//! agent's table-row edit is the editor's, and the AI layer has one mutation
//! path.

use std::path::Path;

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat};
use casual_doc_ai::{Clock, Profile, Session, SessionConfig};
use serde_json::{Value, json};

fn open(bytes: &[u8], name: &str, profile: Profile) -> Session {
    let config = SessionConfig {
        author: "Claude (AI)".to_owned(),
        profile,
        clock: Clock::Fixed("2026-10-28T00:00:00Z".to_owned()),
        ..SessionConfig::default()
    };
    Session::open(bytes, name, config).expect("the fixture opens")
}

fn merged() -> Session {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated/merged-tables.docx"),
    )
    .expect("the committed merged-tables fixture");
    open(&bytes, "merged-tables.docx", Profile::Edit)
}

/// The id of the body's `n`th table (0-based).
fn table_id(session: &Session, n: usize) -> String {
    let blocks = read::read_blocks(session, None, None, Some(20_000), ReadFormat::Json)
        .expect("the body reads");
    blocks["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .filter(|block| block["kind"] == "table")
        .nth(n)
        .expect("that many tables")["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

fn apply(session: &mut Session, edit: Value) {
    let rev = session.revision();
    let plan: EditPlan =
        serde_json::from_value(json!({ "base_revision": rev, "mode": "direct", "edits": [edit] }))
            .expect("a well-formed plan");
    plan::apply(session, &plan).expect("the edit applies");
}

/// Column 1 of the V-MERGE table, top to bottom: (text, verticalMerge).
fn column_one(session: &Session, table: &str) -> Vec<(String, Value)> {
    let read = read::read_table(session, table, None, None).expect("the table reads");
    read["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            let cell = &row["cells"][1];
            (
                cell["text"].as_str().unwrap_or_default().to_owned(),
                cell["verticalMerge"].clone(),
            )
        })
        .collect()
}

#[test]
fn an_agent_row_inserted_inside_a_vertical_merge_grows_the_merge_as_a_persons_does() {
    // The second table's column 1 is one cell merged over rows 0..=2.
    let mut session = merged();
    let table = table_id(&session, 1);
    apply(
        &mut session,
        json!({ "op": "table_insert_row", "table": table, "index": 2 }),
    );
    let column = column_one(&session, &table);
    assert_eq!(column.len(), 5, "one row was added");
    assert_eq!(column[0], ("r0-2c1".to_owned(), json!("restart")));
    // The new row lands inside the merge, so its cell continues it. The old
    // AI lowering cleared `vertical_merge` here and tore the merge in two.
    assert_eq!(
        column[2].1,
        json!("continue"),
        "the new row's cell continues the merge it landed inside: {column:?}"
    );
    assert_eq!(
        column[3].1,
        json!("continue"),
        "the merge still reaches its old last row"
    );
}

#[test]
fn an_agent_deleting_the_top_row_of_a_merge_keeps_the_merged_cells_content() {
    let mut session = merged();
    let table = table_id(&session, 1);
    apply(
        &mut session,
        json!({ "op": "table_delete_row", "table": table, "index": 0 }),
    );
    let column = column_one(&session, &table);
    assert_eq!(column.len(), 3, "one row was removed");
    // `docs/171`: the merged cell's content moves to its first surviving row,
    // which restarts the merge. A raw `DeleteRow` lost the text.
    assert_eq!(
        column[0],
        ("r0-2c1".to_owned(), json!("restart")),
        "the merged cell's text survives on the new top row: {column:?}"
    );
}

#[test]
fn row_edits_cannot_be_suggestions_and_say_why_in_the_registrys_words() {
    let mut session = merged();
    let table = table_id(&session, 1);
    let rev = session.revision();
    let plan: EditPlan = serde_json::from_value(json!({
        "base_revision": rev,
        "edits": [{ "op": "table_insert_row", "table": table, "index": 1 }],
    }))
    .expect("a well-formed plan");
    let refused = plan::apply(&mut session, &plan).expect_err("suggest mode refuses rows");
    assert_eq!(refused.details["firstCode"], "not-trackable");
    let casual_doc_command::Trackable::No(reason) = casual_doc_command::find("table_insert_row")
        .expect("registered")
        .trackable
    else {
        panic!("rows are declared not trackable");
    };
    assert_eq!(refused.details["refusals"][0]["refusal"]["message"], reason);
}

#[test]
fn every_registered_command_is_an_edit_the_plan_accepts_and_nothing_else_is() {
    let text = "The notice period is thirty (30) days.\n";
    let mut session = open(text.as_bytes(), "contract.txt", Profile::Review);
    for command in casual_doc_command::COMMANDS {
        let example: Value = serde_json::from_str(command.example).expect("example is JSON");
        assert_eq!(
            example["op"], command.id,
            "{}'s example names it",
            command.id
        );
        let plan: EditPlan = serde_json::from_value(json!({
            "base_revision": session.revision(),
            "edits": [example],
        }))
        .expect("a well-formed plan");
        // The example's placeholder ids do not resolve; what matters is that
        // the edit PARSED as this command, i.e. the plan's edit enum and the
        // registry agree on its name and its fields.
        if let Err(error) = plan::preview(&session, &plan) {
            assert_ne!(
                error.code, "unknown-edit",
                "{} is not in the plan's vocabulary",
                command.id
            );
            assert!(
                !error.message.contains("is malformed"),
                "{}'s registry example does not match the plan's fields: {}",
                command.id,
                error.message
            );
        }
    }
    let unknown: EditPlan = serde_json::from_value(json!({
        "base_revision": session.revision(),
        "edits": [{ "op": "insert_chart" }],
    }))
    .expect("a well-formed plan");
    let refused = plan::apply(&mut session, &unknown).expect_err("not registered");
    assert_eq!(refused.code, "unknown-edit");
}

/// `docs/181` G6, `docs/147`: the AI layer builds one transaction, in its one
/// choke point, and never applies an operation outside it.
#[test]
fn the_ai_layer_has_one_mutation_path() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut reserves = Vec::new();
    for entry in std::fs::read_dir(&src).expect("sources") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable");
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        assert!(
            !text.contains("casual_doc_edit::apply("),
            "{name} applies an operation outside the transaction envelope"
        );
        for _ in text.matches("Transaction::reserve(") {
            reserves.push(name.clone());
        }
    }
    assert_eq!(
        reserves,
        vec!["session.rs".to_owned()],
        "a transaction is built only by the session's commit choke point"
    );
}
