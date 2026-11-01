// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §3: the editor's table tools as agent commands. Each command
//! here is the twin of editor ids that were listed in `UI_NOT_EXPOSED` until
//! this change (`table.look.*`, `table.splitTable`, `table.insertCells`,
//! `table.deleteCells`, `table.convertToText`, `insert.textToTable`), and each
//! lowers through the same `casual_doc_command::table_tools` function the
//! editor's binding calls, so the guards below check the agent-facing
//! resolution (table id, cell references, anchors), not the lowering's rules,
//! which `table_tools`'s own tests hold.

use std::path::Path;

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat};
use casual_doc_ai::{Profile, Session, SessionConfig};
use casual_doc_model::v1::{BlockNode, Table};
use serde_json::{Value, json};

fn open(name: &str) -> Session {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/generated")
            .join(name),
    )
    .expect("a committed fixture");
    let config = SessionConfig {
        profile: Profile::Edit,
        ..SessionConfig::default()
    };
    Session::open(&bytes, name, config).expect("the fixture opens")
}

/// The plain 5 x 3 table fixture, after one paragraph whose text has commas.
fn plain() -> Session {
    open("oracle-plain-table.docx")
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

/// The id of the body's `n`th table (0-based).
fn table_id(session: &Session, n: usize) -> String {
    let blocks = read::read_blocks(session, None, None, Some(50_000), ReadFormat::Json)
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

/// The body's tables, in order.
fn tables(session: &Session) -> Vec<&Table> {
    session
        .document()
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Table(table) => Some(table.as_ref()),
            _ => None,
        })
        .collect()
}

/// The plain text of cell `(row, cell)` of `table`.
fn cell_text(table: &Table, row: usize, cell: usize) -> String {
    table.rows[row].cells[cell]
        .blocks
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(casual_doc_ai::text::paragraph_text(paragraph)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mutation: drop the `banded_rows` pair from the plan arm's list, and banding
/// stays on; apply the options one `set_look_option` at a time from the
/// original table, and only the last one survives.
#[test]
fn style_options_set_together_land_in_one_step() {
    let mut session = plain();
    let table = table_id(&session, 0);
    let before = tables(&session)[0].properties.look;
    assert!(
        !before.first_column && !before.no_h_band,
        "the premise: first column off, banding on ({before:?})"
    );
    let result = apply(
        &mut session,
        json!({ "op": "table_set_style_options", "table": table,
                "first_column": true, "banded_rows": false, "total_row": true }),
    )
    .expect("the options apply");
    let look = tables(&session)[0].properties.look;
    assert!(look.first_column, "first column on: {look:?}");
    assert!(
        look.no_h_band,
        "banded rows off is stored as noHBand: {look:?}"
    );
    assert!(look.last_row, "total row on: {look:?}");
    assert_eq!(
        result["edits"][0]["operations"], 1,
        "three options are one SetTableProperties: {result}"
    );
}

#[test]
fn style_options_with_nothing_set_are_refused() {
    let mut session = plain();
    let table = table_id(&session, 0);
    let refused = apply(
        &mut session,
        json!({ "op": "table_set_style_options", "table": table }),
    )
    .expect_err("an edit that sets nothing is refused");
    assert_eq!(refused.details["firstCode"], "invalid-argument");
}

/// Mutation: pass `row + 1` to `split_table`, and the first table keeps two
/// rows.
#[test]
fn split_starts_a_second_table_at_the_named_row() {
    let mut session = plain();
    let table = table_id(&session, 0);
    apply(
        &mut session,
        json!({ "op": "table_split", "table": table, "row": 1 }),
    )
    .expect("the split applies");
    let found = tables(&session);
    assert_eq!(found.len(), 2, "two tables now");
    assert_eq!(found[0].rows.len(), 1, "row 0 stays in the first");
    assert_eq!(found[1].rows.len(), 4, "rows 1..5 start the second");
    assert_eq!(cell_text(found[1], 0, 1), "Row 2 column 2");
}

/// Mutation: hand `insert_cells` the shift `down`, and row 0 keeps three cells.
#[test]
fn inserting_cells_right_widens_the_row_at_the_named_cell() {
    let mut session = plain();
    let moved = cell_text(tables(&session)[0], 0, 1);
    let table = table_id(&session, 0);
    apply(
        &mut session,
        json!({ "op": "table_insert_cells", "table": table,
                "from": { "row": 0, "column": 1 }, "shift": "right" }),
    )
    .expect("the cells insert");
    let found = tables(&session)[0];
    assert_eq!(found.rows[0].cells.len(), 4);
    assert_eq!(cell_text(found, 0, 1), "", "the new cell is empty");
    assert_eq!(cell_text(found, 0, 2), moved, "the old cell moved right");
    assert_eq!(found.rows[1].cells.len(), 3, "other rows keep their cells");
}

/// Mutation: ignore `to`, and only row 0's first cell is deleted.
#[test]
fn deleting_a_rectangle_up_moves_the_cells_below_up() {
    let mut session = plain();
    let original = tables(&session)[0].clone();
    let table = table_id(&session, 0);
    apply(
        &mut session,
        json!({ "op": "table_delete_cells", "table": table,
                "from": { "row": 0, "column": 0 }, "to": { "row": 1, "column": 0 },
                "shift": "up" }),
    )
    .expect("the cells delete");
    let found = tables(&session)[0];
    assert_eq!(
        cell_text(found, 0, 0),
        cell_text(&original, 2, 0),
        "row 2 moved up two"
    );
    assert_eq!(
        cell_text(found, 0, 1),
        cell_text(&original, 0, 1),
        "column 1 untouched"
    );
}

/// A shift through a merge is refused in the editor's words.
#[test]
fn a_shift_in_a_merged_table_is_refused_by_name() {
    let mut session = open("merged-tables.docx");
    let table = table_id(&session, 1);
    let refused = apply(
        &mut session,
        json!({ "op": "table_insert_cells", "table": table,
                "from": { "row": 0, "column": 0 }, "shift": "down" }),
    )
    .expect_err("a merged table refuses");
    let message = refused.details["refusals"][0]["refusal"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(message.contains("merged cells"), "{refused:?}");
}

/// Mutation: pass `Separator::Paragraph` whatever was asked, and each cell
/// becomes its own paragraph (15 of them, not 5).
#[test]
fn table_to_text_joins_each_rows_cells_with_the_separator() {
    let mut session = plain();
    let table = table_id(&session, 0);
    apply(
        &mut session,
        json!({ "op": "table_to_text", "table": table, "separator": "other", "character": "|" }),
    )
    .expect("the table converts");
    assert!(tables(&session).is_empty(), "the table is gone");
    let lines: Vec<String> = session
        .document()
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(casual_doc_ai::text::paragraph_text(paragraph)),
            _ => None,
        })
        .collect();
    assert!(
        lines.iter().any(|line| line.starts_with("Row 1 column 1|")),
        "row 1 is one line joined by `|`: {lines:?}"
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.matches('|').count() == 2)
            .count(),
        5,
        "five rows, three cells each: {lines:?}"
    );
}

/// The fixture's first paragraph holds one comma; cut at commas it is one
/// row of two cells. Mutation: resolve `to` instead of `from` as the first
/// paragraph when `to` is absent (it is), and the edit is refused.
#[test]
fn text_to_table_cuts_a_paragraph_at_its_commas() {
    let mut session = plain();
    let first = read::read_blocks(&session, None, None, Some(50_000), ReadFormat::Json)
        .expect("the body reads")["blocks"][0]["id"]
        .as_str()
        .expect("a paragraph id")
        .to_owned();
    let text = casual_doc_ai::text::paragraph_text(match &session.document().body()[0] {
        BlockNode::Paragraph(paragraph) => paragraph,
        _ => panic!("the fixture opens with a paragraph"),
    });
    let commas = text.matches(',').count();
    assert!(
        commas >= 1,
        "the premise: the first paragraph has a comma: {text:?}"
    );
    apply(
        &mut session,
        json!({ "op": "text_to_table", "from": { "node": first }, "separator": "comma" }),
    )
    .expect("the text converts");
    let BlockNode::Table(table) = &session.document().body()[0] else {
        panic!("the first block is a table now");
    };
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells.len(), commas + 1);
    assert_eq!(table.grid.len(), commas + 1);
}

/// `docs/183` §23: `insert_table` is trackable. Suggested, it is Word's tracked table — every
/// row `w:trPr/w:ins` — listed as ONE review item a reviewer accepts or rejects.
#[test]
fn a_suggested_insert_table_is_one_tracked_table() {
    let mut session = plain();
    let blocks = read::read_blocks(&session, None, None, Some(50_000), ReadFormat::Json)
        .expect("the body reads");
    let first = blocks["blocks"][0]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let before = tables(&session).len();
    let plan: EditPlan = serde_json::from_value(json!({
        "base_revision": session.revision(),
        "mode": "suggest",
        "edits": [{ "op": "insert_table", "after": { "node": first }, "rows": 2, "columns": 3 }],
    }))
    .expect("a well-formed plan");
    plan::apply(&mut session, &plan).expect("suggest mode inserts a tracked table");
    let all = tables(&session);
    assert_eq!(all.len(), before + 1);
    let inserted = all[0];
    assert!(
        inserted.rows.iter().all(|row| row
            .properties
            .row_revision
            .as_ref()
            .is_some_and(|mark| mark.kind == casual_doc_model::v1::MarkRevisionKind::Insertion)),
        "every row is a tracked insertion"
    );
    let items = casual_doc_review::body::build_review_revision_summaries(session.document());
    let tables: Vec<_> = items
        .iter()
        .filter(|item| item.kind == "table_insertion")
        .collect();
    assert_eq!(tables.len(), 1, "one card for the table: {items:?}");
}
