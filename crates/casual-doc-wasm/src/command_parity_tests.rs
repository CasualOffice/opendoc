// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §3, guard G7: the editor's wasm method and the capability
//! registry's lowering produce the **same operations** on the same document
//! and the same resolved input, family by family (tables, character and
//! paragraph formatting, lists).
//!
//! Each case clones the document's id generator, asks `casual_doc_command`
//! for the operations, then drives the editor's method on the real document
//! and reads the commit it made back off the revision log. Equal lists mean an
//! agent's edit and a person's are one edit. A second, inline lowering in a
//! wasm method (the shape of the `docs/181` §0 row divergence) breaks this the
//! moment it differs; [`the_editor_builds_no_moved_lowering_itself`] refuses
//! the inline copy even before it differs.

use casual_doc_command::paragraph::{ParagraphChange, set_properties, untracked};
use casual_doc_command::{format, lists, table as command};
use casual_doc_edit::Operation;
use casual_doc_model::v1::{BlockNode, HighlightColor, NumberingRef, ParagraphProperties, Table};
use casual_doc_model::{IdGenerator, NodeId};

use crate::{WasmDocument, open_document};

const MERGED: &[u8] = include_bytes!("../../../fixtures/generated/merged-tables.docx");

/// The operations of the newest commit: what the editor's method applied.
fn committed(d: &WasmDocument) -> Vec<Operation> {
    d.log
        .commits()
        .last()
        .expect("the method committed")
        .operations()
        .to_vec()
}

/// A generator that will mint exactly what the editor's next action mints.
fn ids(d: &WasmDocument) -> IdGenerator {
    d.edit_ids.clone()
}

fn mint(ids: &mut IdGenerator) -> impl FnMut() -> Option<NodeId> + '_ {
    move || ids.next_id().ok()
}

/// The body's top-level tables, in order.
fn tables(d: &WasmDocument) -> Vec<Table> {
    d.document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Table(table) => Some(table.as_ref().clone()),
            _ => None,
        })
        .collect()
}

/// The first paragraph of physical cell (`row`, `cell`).
fn cell_paragraph(table: &Table, row: usize, cell: usize) -> String {
    command::first_paragraph_of_cell(&table.rows[row].cells[cell])
        .expect("a cell paragraph")
        .to_string()
}

/// The body paragraphs of a text document, in order.
fn paragraphs(d: &WasmDocument) -> Vec<NodeId> {
    d.document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id),
            _ => None,
        })
        .collect()
}

fn properties(d: &WasmDocument, node: NodeId) -> ParagraphProperties {
    casual_doc_edit::paragraph_properties(&d.document, node).expect("a paragraph")
}

fn text_document(text: &str) -> (WasmDocument, Vec<NodeId>) {
    let d = open_document(text.as_bytes()).expect("a text document opens");
    let nodes = paragraphs(&d);
    (d, nodes)
}

#[test]
fn table_structure_is_one_lowering_for_the_editor_and_agents() {
    // V-MERGE: column 1 is one cell merged over rows 0..=2, so every edit
    // here takes the merged-cell (`ReplaceTable`) path of `docs/171`.
    let mut d = open_document(MERGED).expect("the fixture opens");
    let merged = tables(&d)[1].clone();

    // Insert a column right of the merged cell.
    let mut expected_ids = ids(&d);
    let expected = command::insert_column_beside(&merged, 0, 1, true, &mut mint(&mut expected_ids))
        .expect("lowers")
        .operations;
    d.insert_column_inner(&cell_paragraph(&merged, 0, 1), true)
        .expect("the editor inserts");
    assert_eq!(
        committed(&d),
        expected,
        "insert column: editor and command differ"
    );

    // Delete the column of the merged cell.
    let merged = tables(&d)[1].clone();
    let expected = command::delete_column_of_cell(&merged, 1, 1)
        .expect("lowers")
        .operations;
    d.delete_column_inner(&cell_paragraph(&merged, 1, 1))
        .expect("the editor deletes");
    assert_eq!(
        committed(&d),
        expected,
        "delete column: editor and command differ"
    );

    // Merge two cells: the rectangle grows to whole merged cells either way.
    let merged = tables(&d)[1].clone();
    let rect = command::cell_rect(&merged, (0, 0), (1, 0)).expect("a rectangle");
    let mut expected_ids = ids(&d);
    let expected = command::merge(&merged, rect, &mut mint(&mut expected_ids))
        .expect("lowers")
        .operations;
    d.merge_table_cell_range_inner(
        &cell_paragraph(&merged, 0, 0),
        &cell_paragraph(&merged, 1, 0),
    )
    .expect("the editor merges");
    assert_eq!(committed(&d), expected, "merge: editor and command differ");

    // Unmerge from a covered row of the merged cell.
    let merged = tables(&d)[1].clone();
    let mut expected_ids = ids(&d);
    let expected = command::unmerge(&merged, 1, 0, &mut mint(&mut expected_ids))
        .expect("lowers")
        .operations;
    d.split_merged_cell_inner(&cell_paragraph(&merged, 1, 0), None, None)
        .expect("the editor unmerges");
    assert_eq!(
        committed(&d),
        expected,
        "unmerge: editor and command differ"
    );

    // Cell shading over a rectangle, each merged cell once.
    let merged = tables(&d)[1].clone();
    let cells = command::range_cells(&merged, (0, 0), (1, 1)).expect("cells");
    let expected = command::set_cell_properties(cells, |p| {
        p.shading.fill = Some(casual_doc_model::v1::RgbColor { r: 1, g: 2, b: 3 });
    });
    d.set_cell_shading_range(
        &cell_paragraph(&merged, 0, 0),
        &cell_paragraph(&merged, 1, 1),
        1,
        2,
        3,
        false,
    )
    .expect("the editor shades");
    assert_eq!(
        committed(&d),
        expected,
        "cell shading: editor and command differ"
    );

    // Delete the whole table.
    let merged = tables(&d)[1].clone();
    d.delete_table(&cell_paragraph(&merged, 0, 0))
        .expect("the editor deletes the table");
    assert_eq!(committed(&d), command::delete_table(merged.id));
}

#[test]
fn a_new_table_and_its_columns_are_one_lowering_for_the_editor_and_agents() {
    let (mut d, nodes) = text_document("alpha\nbeta\n");
    let c = &d.default_config;
    let width = c.page_size.width.raw() - c.margin_start.raw() - c.margin_end.raw();
    let index = command::body_index_after(&d.document, nodes[0]);
    let mut expected_ids = ids(&d);
    let expected = command::insert_table(index, 3, 2, width, &mut mint(&mut expected_ids))
        .expect("lowers")
        .operations;
    d.insert_table(&nodes[0].to_string(), 3, 2)
        .expect("the editor inserts a table");
    assert_eq!(
        committed(&d),
        expected,
        "insert table: editor and command differ"
    );

    // A regular table takes the fine-grained column operations.
    let table = tables(&d)[0].clone();
    let mut expected_ids = ids(&d);
    let expected = command::insert_column_beside(&table, 1, 0, false, &mut mint(&mut expected_ids))
        .expect("lowers")
        .operations;
    assert!(matches!(expected[..], [Operation::InsertColumn { .. }]));
    d.insert_column_inner(&cell_paragraph(&table, 1, 0), false)
        .expect("the editor inserts");
    assert_eq!(committed(&d), expected, "regular insert column differs");

    let table = tables(&d)[0].clone();
    let expected = command::delete_column_of_cell(&table, 2, 2)
        .expect("lowers")
        .operations;
    assert!(matches!(expected[..], [Operation::DeleteColumn { .. }]));
    d.delete_column_inner(&cell_paragraph(&table, 2, 2))
        .expect("the editor deletes");
    assert_eq!(committed(&d), expected, "regular delete column differs");
}

#[test]
fn character_formatting_is_one_lowering_for_the_editor_and_agents() {
    let (mut d, nodes) = text_document("alpha\nbeta\n");
    let (a, b) = (nodes[0], nodes[1]);
    let (sa, sb) = (a.to_string(), b.to_string());
    // A selection from inside the first paragraph into the second.
    let ranges = [(a, 2, 5), (b, 0, 3)];

    let bold = casual_doc_edit::FormatDelta {
        bold: Some(true),
        ..Default::default()
    };
    d.format_selection(&sa, 2, &sb, 3, Some(true), None, None, None)
        .expect("bold");
    assert_eq!(committed(&d), format::format_ranges(&ranges, &bold), "bold");

    let gray = casual_doc_edit::FormatDelta {
        highlight: Some(format::parse_highlight("gray").expect("a Word highlight")),
        ..Default::default()
    };
    assert_eq!(gray.highlight, Some(HighlightColor::DarkGray));
    d.set_highlight(&sa, 2, &sb, 3, "gray").expect("highlight");
    assert_eq!(
        committed(&d),
        format::format_ranges(&ranges, &gray),
        "highlight"
    );

    let size = casual_doc_edit::FormatDelta {
        size_half_points: Some(format::half_points(10.5).expect("a size")),
        ..Default::default()
    };
    d.set_font_size(&sa, 2, &sb, 3, 10.5).expect("size");
    assert_eq!(committed(&d), format::format_ranges(&ranges, &size), "size");

    d.clear_formatting(&sa, 0, 5).expect("clear");
    assert_eq!(
        committed(&d),
        format::clear_ranges(&[(a, 0, 5)]),
        "clear formatting"
    );
}

#[test]
fn paragraph_formatting_is_one_lowering_for_the_editor_and_agents() {
    let (mut d, nodes) = text_document("alpha\nbeta\ngamma\n");
    let (a, b) = (nodes[0], nodes[1]);
    let (sa, sb) = (a.to_string(), b.to_string());
    let expect = |d: &WasmDocument, change: ParagraphChange| {
        set_properties(
            Vec::new(),
            vec![(a, properties(d, a)), (b, properties(d, b))],
            |p| change.apply(p),
            &mut untracked,
        )
        .expect("lowers")
    };
    type Step = (ParagraphChange, fn(&mut WasmDocument, &str, &str));
    let steps: [Step; 8] = [
        (
            ParagraphChange::Alignment(casual_doc_command::paragraph::alignment("center")),
            |d, a, b| drop(d.set_alignment(a, 0, b, 1, "center").expect("align")),
        ),
        (ParagraphChange::LineSpacingMultiple(150), |d, a, b| {
            drop(d.set_line_spacing(a, 0, b, 1, 150).expect("spacing"));
        }),
        (
            ParagraphChange::LineSpacingFixed {
                twips: 360,
                at_least: true,
            },
            |d, a, b| {
                drop(
                    d.set_line_spacing_exact(a, 0, b, 1, 360, true)
                        .expect("exact"),
                )
            },
        ),
        (ParagraphChange::SpaceBefore(240), |d, a, b| {
            drop(d.set_space_before(a, 0, b, 1, 240).expect("before"));
        }),
        (ParagraphChange::SpaceAfter(-1), |d, a, b| {
            drop(d.set_space_after(a, 0, b, 1, -1).expect("after"));
        }),
        (ParagraphChange::AdjustIndent(720), |d, a, b| {
            drop(d.adjust_indent(a, 0, b, 1, 720).expect("indent"));
        }),
        (ParagraphChange::EndIndent(360), |d, a, b| {
            drop(d.set_right_indent(a, 0, b, 1, 360).expect("right"));
        }),
        (ParagraphChange::FirstLineIndent(-360), |d, a, b| {
            drop(d.set_first_line_indent(a, 0, b, 1, -360).expect("hanging"));
        }),
    ];
    for (change, drive) in steps {
        let expected = expect(&d, change);
        drive(&mut d, &sa, &sb);
        assert_eq!(
            committed(&d),
            expected,
            "{change:?}: editor and command differ"
        );
    }
}

#[test]
fn lists_are_one_lowering_for_the_editor_and_agents() {
    let (mut d, nodes) = text_document("one\ntwo\nthree\nfour\n");
    let (first, last) = (nodes[0].to_string(), nodes[3].to_string());

    // Numbering on: the definition, installed first, then each paragraph.
    let mut expected_ids = ids(&d);
    let (instance, install) =
        lists::new_definition(true, &mut mint(&mut expected_ids)).expect("a definition");
    let targets = nodes[..4]
        .iter()
        .map(|n| (*n, properties(&d, *n)))
        .collect();
    let expected = set_properties(
        install,
        targets,
        |p| ParagraphChange::Numbering(Some(NumberingRef { instance, level: 0 })).apply(p),
        &mut untracked,
    )
    .expect("lowers");
    d.toggle_list(&first, 0, &last, 0, "numbered")
        .expect("numbering on");
    assert_eq!(
        committed(&d),
        expected,
        "toggle list: editor and command differ"
    );

    // Demote the second item.
    let two = nodes[1];
    let expected = set_properties(
        Vec::new(),
        vec![(two, properties(&d, two))],
        |p| ParagraphChange::ListLevel(1).apply(p),
        &mut untracked,
    )
    .expect("lowers");
    d.adjust_list_level(&two.to_string(), 0, &two.to_string(), 0, 1)
        .expect("demote");
    assert_eq!(
        committed(&d),
        expected,
        "list level: editor and command differ"
    );

    // Restart at the third item, then continue it again.
    let three = nodes[2];
    let ordered: Vec<NodeId> = d
        .ordered_paragraphs()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let mut expected_ids = ids(&d);
    let expected = lists::restart(
        &d.document,
        &ordered,
        three,
        1,
        &mut mint(&mut expected_ids),
    )
    .expect("lowers");
    d.restart_list(&three.to_string()).expect("restart");
    assert_eq!(
        committed(&d),
        expected,
        "restart: editor and command differ"
    );

    let expected = lists::continue_numbering(&d.document, &ordered, three).expect("lowers");
    d.continue_list(&three.to_string()).expect("continue");
    assert_eq!(
        committed(&d),
        expected,
        "continue: editor and command differ"
    );
}

/// The production lines of the editor's sources: a test module, whole-file
/// (`*_tests.rs`) or trailing (`#[cfg(test)] mod …`), is not production.
fn production_lines() -> Vec<(String, usize, String)> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&src)
        .expect("the editor's sources")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with("_tests.rs"))
        })
        .collect();
    files.sort();
    assert!(
        files.iter().any(|path| path.ends_with("lib.rs")),
        "the scan must read lib.rs or it passes for the wrong reason"
    );
    for path in files {
        let text = std::fs::read_to_string(&path).expect("readable");
        let lines: Vec<&str> = text.lines().collect();
        let end = (0..lines.len())
            .find(|&index| {
                lines[index].trim_end() == "#[cfg(test)]"
                    && lines
                        .get(index + 1)
                        .is_some_and(|next| next.starts_with("mod "))
            })
            .unwrap_or(lines.len());
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        for (number, line) in lines[..end].iter().enumerate() {
            out.push((name.clone(), number + 1, (*line).to_owned()));
        }
    }
    out
}

/// `docs/181` G7, the source half (the precedent is
/// `casual-doc-review/tests/one_lowering.rs`): the lowerings moved into
/// `casual_doc_command` are not rebuilt inside a wasm method. A call to a
/// `table_structure` rule, or a constructed table-structure, cell-format or
/// clear-formatting operation, in the editor's production code is a second
/// lowering beside the registry's.
#[test]
fn the_editor_builds_no_moved_lowering_itself() {
    const CALLS: &[&str] = &[
        "table_structure::insert_row(",
        "table_structure::delete_rows(",
        "table_structure::insert_column(",
        "table_structure::delete_columns(",
        "table_structure::merge_rect(",
        "table_structure::unmerge(",
        "fn list_level(",
    ];
    const CONSTRUCTED: &[&str] = &[
        "Operation::InsertRow {",
        "Operation::DeleteRow {",
        "Operation::InsertColumn {",
        "Operation::DeleteColumn {",
        "Operation::InsertTable {",
        "Operation::DeleteTable {",
        "Operation::SetTableCellProperties {",
        "Operation::ClearFormatting {",
    ];
    let mut found = Vec::new();
    for (file, number, line) in production_lines() {
        let code = line.split("//").next().unwrap_or("").trim();
        let call = CALLS.iter().any(|needle| code.contains(needle));
        // A match arm (`Operation::X { .. }`, `| Operation::X { range }`,
        // `Operation::X { table } =>`) reads an operation; only a construction
        // builds one.
        let constructed = CONSTRUCTED.iter().any(|needle| code.contains(needle))
            && !code.starts_with('|')
            && !code.contains("=>")
            && !code.contains("{ ..");
        if call || constructed {
            found.push(format!("{file}:{number}: {code}"));
        }
    }
    assert!(
        found.is_empty(),
        "the editor lowers a registry command itself; call casual_doc_command instead \
         (docs/181 G7):\n{}",
        found.join("\n")
    );
}
