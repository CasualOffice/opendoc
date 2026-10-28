// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §3, guard G7: the editor's wasm method and the capability
//! registry's lowering produce the **same operations** on the same document
//! and the same resolved input, family by family (tables, character and
//! paragraph formatting, lists, links, fields, notes, sections, objects,
//! charts, and the reference family: tables of contents, captions and
//! cross-references, whose resolved input includes what the editor's layout
//! lends them).
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
// The step 4-6 families (lane families-b), on their own line per the
// parallel-lane import rule.
use casual_doc_command::{breaks, fields, links, notes, objects, sections};
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::v1::{BlockNode, HighlightColor, NumberingRef, ParagraphProperties, Table};
use casual_doc_model::v1::{BreakKind, NoteKind};
use casual_doc_model::{IdGenerator, NodeId};

use crate::{WasmDocument, open_document};
// Lane references (docs/181 §12), on their own lines per the parallel-lane
// import rule.
use crate::toc::EditorFacts;
use casual_doc_command::references::{self, TocUpdate};
use casual_doc_edit::references::{
    CaptionChapter, CaptionNumberFormat, CaptionPosition, CaptionSeparator, CaptionSpec,
    ReferenceTo,
};
use casual_doc_model::v1::InlineNode;
use std::collections::HashMap;

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

/// A one-section LibreOffice-written document: body paragraphs, footnotes,
/// a `w:sectPr`.
const SECTIONED: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx");

/// The first body paragraph that holds text, as an id string.
fn first_text_paragraph(d: &WasmDocument) -> NodeId {
    d.document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph)
                if !crate::node_plain_text(&paragraph.inlines).is_empty() =>
            {
                Some(paragraph.id)
            }
            _ => None,
        })
        .expect("a text paragraph")
}

#[test]
fn links_bookmarks_and_fields_are_one_lowering_for_the_editor_and_agents() {
    let (mut d, nodes) = text_document("alpha beta\ngamma delta\n");
    let (a, b) = (nodes[0], nodes[1]);
    let (sa, sb) = (a.to_string(), b.to_string());

    for (target, tooltip) in [
        ("https://example.com/", Some("Example".to_owned())),
        ("#Clause", None),
    ] {
        let mut expected_ids = ids(&d);
        let expected = links::set_hyperlink(
            a,
            0,
            5,
            target,
            tooltip.clone(),
            &mut mint(&mut expected_ids),
        )
        .expect("lowers");
        d.set_hyperlink(&sa, 0, 5, target.to_owned(), tooltip)
            .expect("the editor links");
        assert_eq!(
            committed(&d),
            expected,
            "link {target}: editor and command differ"
        );
    }
    d.remove_hyperlink(&sa, 0, 5).expect("the editor unlinks");
    assert_eq!(
        committed(&d),
        links::remove_hyperlink(a, 0, 5),
        "unlink: editor and command differ"
    );

    // A bookmark across two paragraphs, then renamed and deleted.
    let mut expected_ids = ids(&d);
    let expected = links::create_bookmark(
        "Mark".to_owned(),
        Pos::new(a, 6),
        Pos::new(b, 5),
        &mut mint(&mut expected_ids),
    )
    .expect("lowers");
    d.create_bookmark(&sa, 6, &sb, 5, "Mark".to_owned())
        .expect("the editor bookmarks");
    assert_eq!(
        committed(&d),
        expected,
        "bookmark: editor and command differ"
    );
    let Some(Operation::CreateBookmark { bookmark, .. }) = committed(&d).first().cloned() else {
        panic!("the bookmark was created");
    };
    d.rename_bookmark(&bookmark.node_id().to_string(), "Renamed".to_owned())
        .expect("renames");
    assert_eq!(
        committed(&d),
        links::rename_bookmark(bookmark, "Renamed".to_owned())
    );
    d.delete_bookmark(&bookmark.node_id().to_string())
        .expect("deletes");
    assert_eq!(committed(&d), links::delete_bookmark(bookmark));

    for kind in fields::KINDS {
        let mut expected_ids = ids(&d);
        let expected = fields::insert_field(
            Pos::new(b, 0),
            kind,
            "cached".to_owned(),
            &mut mint(&mut expected_ids),
        )
        .expect("lowers");
        d.insert_field(&sb, 0, kind, Some("cached".to_owned()))
            .expect("the editor inserts a field");
        assert_eq!(
            committed(&d),
            expected,
            "field {kind}: editor and command differ"
        );
    }
}

#[test]
fn notes_and_breaks_are_one_lowering_for_the_editor_and_agents() {
    let mut d = open_document(SECTIONED).expect("the fixture opens");
    let p = first_text_paragraph(&d);
    let sp = p.to_string();

    for kind in [NoteKind::Footnote, NoteKind::Endnote] {
        let mut expected_ids = ids(&d);
        let expected = notes::insert_note(kind, Pos::new(p, 1), None, &mut mint(&mut expected_ids))
            .expect("lowers")
            .operations;
        match kind {
            NoteKind::Footnote => drop(d.insert_footnote(&sp, 1).expect("footnote")),
            NoteKind::Endnote => drop(d.insert_endnote(&sp, 1).expect("endnote")),
        }
        assert_eq!(
            committed(&d),
            expected,
            "{kind:?}: editor and command differ"
        );
    }

    let mut expected_ids = ids(&d);
    let expected = breaks::insert_break(
        &d.document,
        Pos::new(p, 2),
        BreakKind::Line,
        &mut mint(&mut expected_ids),
    )
    .expect("lowers");
    d.insert_line_break(&sp, 2).expect("line break");
    assert_eq!(
        committed(&d),
        expected,
        "line break: editor and command differ"
    );

    for (name, kind) in [("page", BreakKind::Page), ("column", BreakKind::Column)] {
        let mut expected_ids = ids(&d);
        let expected = breaks::insert_break(
            &d.document,
            Pos::new(p, 2),
            kind,
            &mut mint(&mut expected_ids),
        )
        .expect("lowers");
        d.insert_break_inner(&sp, 2, name).expect("break");
        assert_eq!(
            committed(&d),
            expected,
            "{name} break: editor and command differ"
        );
    }

    let mut expected_ids = ids(&d);
    let expected =
        breaks::insert_blank_page(&d.document, Pos::new(p, 2), &mut mint(&mut expected_ids))
            .expect("lowers");
    d.insert_blank_page(&sp, 2).expect("blank page");
    assert_eq!(
        committed(&d),
        expected,
        "blank page: editor and command differ"
    );

    let mut expected_ids = ids(&d);
    let expected = breaks::section_break(
        &d.document,
        Pos::new(p, 1),
        casual_doc_model::v1::SectionType::Continuous,
        &mut mint(&mut expected_ids),
    )
    .expect("lowers")
    .operations;
    d.insert_section_break_inner(&sp, 1, "continuous")
        .expect("section break");
    assert_eq!(
        committed(&d),
        expected,
        "section break: editor and command differ"
    );
}

#[test]
fn page_setup_and_running_content_are_one_lowering_for_the_editor_and_agents() {
    let mut d = open_document(SECTIONED).expect("the fixture opens");
    let p = first_text_paragraph(&d).to_string();
    let first = d.document.definitions().sections[0].clone();

    // Page setup: the dialog's payload, widened, written back.
    let mut payload: serde_json::Value =
        serde_json::from_str(&d.page_setup()).expect("page setup JSON");
    payload["pageSize"]["widthTwips"] = serde_json::json!(15_840);
    payload["pageSize"]["heightTwips"] = serde_json::json!(12_240);
    payload["orientation"] = serde_json::json!("landscape");
    payload
        .as_object_mut()
        .expect("an object")
        .remove("columns");
    let expected = sections::set_geometry(
        &d.document,
        first.id,
        casual_doc_model::v1::PageSize {
            width_twips: 15_840,
            height_twips: 12_240,
        },
        first.page_margins,
        Some(casual_doc_model::v1::PageOrientation::Landscape),
        None,
    );
    d.set_page_setup(&payload.to_string()).expect("page setup");
    assert_eq!(
        committed(&d),
        expected,
        "page setup: editor and command differ"
    );

    let mut payload: serde_json::Value =
        serde_json::from_str(&d.line_numbering(&p)).expect("line numbering JSON");
    payload["countBy"] = serde_json::json!(5);
    payload["start"] = serde_json::json!(1);
    d.set_line_numbering(&payload.to_string())
        .expect("line numbers");
    assert_eq!(
        committed(&d),
        sections::set_line_numbering(
            first.id,
            casual_doc_model::v1::LineNumbering {
                count_by: Some(5),
                start: Some(1),
                distance: None,
                restart: None,
            }
        ),
        "line numbering: editor and command differ"
    );

    let layout = serde_json::json!({
        "section": first.id.node_id().to_string(),
        "verticalAlignment": "center",
        "pageNumberFormat": "lowerRoman",
        "pageNumberStart": 3,
    });
    d.set_section_layout(&layout.to_string())
        .expect("section layout");
    assert_eq!(
        committed(&d),
        sections::set_section_layout(
            first.id,
            Some(casual_doc_model::v1::NumberFormat::LowerRoman),
            Some(3),
            Some(casual_doc_model::v1::PageVerticalAlignment::Center),
        ),
        "section layout: editor and command differ"
    );

    let mut payload: serde_json::Value =
        serde_json::from_str(&d.watermark(&p)).expect("watermark JSON");
    payload["kind"] = serde_json::json!("text");
    payload["text"] = serde_json::json!("DRAFT");
    payload["font"] = serde_json::json!("Arial");
    payload["color"] = serde_json::json!("#c0c0c0");
    payload["layout"] = serde_json::json!("horizontal");
    let expected = sections::set_watermark(
        &d.document,
        Some(Box::new(sections::text_watermark(
            "DRAFT".to_owned(),
            Some("Arial"),
            None,
            casual_doc_command::objects::opaque(casual_doc_model::v1::RgbColor {
                r: 0xc0,
                g: 0xc0,
                b: 0xc0,
            }),
            false,
            false,
            casual_doc_model::v1::WatermarkLayout::Horizontal,
            payload["semiTransparent"].as_bool().unwrap_or(false),
        ))),
    );
    d.set_watermark(&payload.to_string()).expect("watermark");
    assert_eq!(
        committed(&d),
        expected,
        "watermark: editor and command differ"
    );

    // A header for page 1, before any page variant exists.
    let mut expected_ids = ids(&d);
    let expected = sections::create_running_content(
        first.id,
        casual_doc_edit::RunningRegion::Header,
        casual_doc_model::v1::HeaderFooterKind::Default,
        &mut mint(&mut expected_ids),
    )
    .expect("lowers")
    .operations;
    d.create_running_content_inner("header", 1)
        .expect("a header");
    assert_eq!(committed(&d), expected, "header: editor and command differ");

    d.set_first_page_variant(true, 1).expect("first page");
    assert_eq!(committed(&d), sections::set_title_page(first.id, true));
    d.set_even_odd_variant(true).expect("odd and even");
    assert_eq!(committed(&d), sections::set_even_and_odd(true));
}

/// The first inline object `InsertInlineObject` placed in a commit.
fn inserted_object(ops: &[Operation]) -> casual_doc_model::v1::InlineNode {
    ops.iter()
        .find_map(|op| match op {
            Operation::InsertInlineObject { node, .. } => Some(node.as_ref().clone()),
            _ => None,
        })
        .expect("an inserted object")
}

#[test]
fn pictures_and_objects_are_one_lowering_for_the_editor_and_agents() {
    use casual_doc_model::v1::{GroupChild, InlineNode, Rgba};

    let (mut d, nodes) = text_document("alpha\nbeta\n");
    let a = nodes[0];
    let sa = a.to_string();
    let png = objects::decode_base64(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
        1024,
    )
    .expect("a PNG");

    let mut expected_ids = ids(&d);
    let picture = objects::insert_picture(
        // At the paragraph's end, so deleting it later leaves one run (a
        // deletion that would leave two mergeable runs is the host's to fix).
        Pos::new(a, 5),
        "image/png",
        914_400,
        457_200,
        &mut mint(&mut expected_ids),
    )
    .expect("lowers");
    d.insert_image(
        &sa,
        5,
        png.clone(),
        914_400.0,
        457_200.0,
        "image/png".to_owned(),
    )
    .expect("the editor inserts a picture");
    assert_eq!(
        committed(&d),
        picture.operations,
        "picture: editor and command differ"
    );
    assert_eq!(
        d.resources.get(&picture.part_name),
        Some(png.as_slice()),
        "the bytes are written under the part the lowering names"
    );
    let image = picture.drawing;
    let si = image.to_string();

    d.set_object_extent(&si, 457_200.0, 228_600.0)
        .expect("resize");
    assert_eq!(
        committed(&d),
        objects::set_extent(image, 457_200, 228_600).expect("lowers")
    );
    d.set_image_crop(&si, Some(vec![0.1, 0.0, 0.25, 0.0]))
        .expect("crop");
    assert_eq!(
        committed(&d),
        objects::set_crop(image, Some(objects::crop([0.1, 0.0, 0.25, 0.0])))
    );
    d.set_object_descr(&si, Some("A dot".to_owned()))
        .expect("alt text");
    assert_eq!(
        committed(&d),
        objects::set_alt_text(image, Some("A dot".to_owned()))
    );
    let navy = Rgba {
        r: 0x1f,
        g: 0x38,
        b: 0x64,
        a: 255,
    };
    let expected = objects::set_shape_outline(&d.document, image, Some(navy), Some(19_050.0));
    d.set_shape_outline(&si, Some("#1f3864".to_owned()), Some(19_050.0))
        .expect("outline");
    assert_eq!(
        committed(&d),
        expected,
        "outline: editor and command differ"
    );
    d.delete_object(&si).expect("delete");
    assert_eq!(committed(&d), objects::delete_object(image));

    // A floating text box's wrap.
    d.insert_text_box(&sa, 0).expect("a text box");
    let InlineNode::TextBox(text_box) = inserted_object(&committed(&d)) else {
        panic!("a text box");
    };
    let expected = objects::set_wrap(&d.document, text_box.id, "tight").expect("lowers");
    d.set_object_wrap(&text_box.id.to_string(), "tight")
        .expect("wrap");
    assert_eq!(committed(&d), expected, "wrap: editor and command differ");

    // A shape's fill.
    d.insert_shape(&sa, 0, "rect").expect("a shape");
    let InlineNode::Group(group) = inserted_object(&committed(&d)) else {
        panic!("a shape group");
    };
    let Some(GroupChild::Shape(shape)) = group.children.first() else {
        panic!("a shape child");
    };
    d.set_shape_fill(&shape.id.to_string(), Some("#fff2cc".to_owned()))
        .expect("fill");
    assert_eq!(
        committed(&d),
        objects::set_shape_fill(
            shape.id,
            Some(Rgba {
                r: 0xff,
                g: 0xf2,
                b: 0xcc,
                a: 255,
            })
        ),
        "fill: editor and command differ"
    );
}

#[test]
fn chart_insertion_is_one_lowering_for_the_editor_and_agents() {
    let (mut d, nodes) = text_document("alpha\nbeta\n");
    let a = nodes[0];
    for kind in casual_doc_command::charts::KINDS {
        let mut expected_ids = ids(&d);
        let expected = casual_doc_command::charts::insert_chart(
            Pos::new(a, 5),
            kind,
            &mut mint(&mut expected_ids),
        )
        .expect("lowers");
        d.insert_chart(&a.to_string(), 5, kind)
            .expect("the editor inserts a chart");
        assert_eq!(
            committed(&d),
            expected,
            "{kind} chart: editor and command differ"
        );
    }
}

/// The production lines of the editor's sources: a test module, whole-file
/// (`*_tests.rs`) or trailing (`#[cfg(test)] mod …`), is not production.
/// A body of headings, captions' targets and text, imported from real markup so
/// the headings carry an `w:outlineLvl` the cascade has to resolve: "Preface",
/// "Chapter A" (level 1), "The wiring", "Chapter B" (level 2), "The layout".
fn reference_document() -> WasmDocument {
    let document = casual_doc_import::import_main_document_xml(
        br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>Preface</w:t></w:r></w:p>
            <w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Chapter A</w:t></w:r></w:p>
            <w:p><w:r><w:t>The wiring</w:t></w:r></w:p>
            <w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>Chapter B</w:t></w:r></w:p>
            <w:p><w:r><w:t>The layout</w:t></w:r></w:p>
        </w:body></w:document>"#,
        casual_doc_import::ImportConfig::default(),
    )
    .expect("import")
    .document;
    crate::tests::wasm_document(document)
}

/// What an agent lends a table of contents, built here rather than through
/// the editor's glue, so the glue's own choices are what is compared: the
/// page label of every laid-out block, and the content width of the
/// document's first section (what `casual-doc-ai` computes).
fn toc_facts(d: &WasmDocument) -> EditorFacts<'_> {
    let definitions = d.document.definitions();
    EditorFacts::with_pages(
        definitions,
        casual_doc_layout::paginate::first_page_labels(d.layout.resident(), &definitions.sections),
    )
}

fn content_width(d: &WasmDocument) -> i32 {
    let page = casual_doc_layout::document_layout::document_page_config(&d.document);
    page.page_size.width.raw() - page.margin_start.raw() - page.margin_end.raw()
}

#[test]
fn tables_of_contents_are_one_lowering_for_the_editor_and_agents() {
    let mut d = reference_document();
    let preface = paragraphs(&d)[0];

    for levels in [1_u32, 0] {
        let mut expected_ids = ids(&d);
        let expected = {
            let facts = toc_facts(&d);
            references::insert_table_of_contents(
                &d.document,
                &facts,
                preface,
                references::toc_level(levels),
                content_width(&d),
                &mut mint(&mut expected_ids),
            )
            .expect("lowers")
        };
        d.insert_table_of_contents_inner(&preface.to_string(), levels)
            .expect("the editor inserts a table of contents");
        assert_eq!(
            committed(&d),
            expected.operations,
            "table of contents ({levels} levels): editor and command differ"
        );
        drop(d.undo().expect("undo"));
    }

    d.insert_table_of_contents_inner(&preface.to_string(), 3)
        .expect("a table to update");
    for (mode, name) in [
        (TocUpdate::PageNumbers, "pageNumbers"),
        (TocUpdate::Entire, "entire"),
    ] {
        let mut expected_ids = ids(&d);
        let expected = {
            let facts = toc_facts(&d);
            references::update_table_of_contents(
                &d.document,
                &facts,
                None,
                mode,
                content_width(&d),
                &mut mint(&mut expected_ids),
            )
            .expect("lowers")
        };
        d.update_table_of_contents_inner("", name)
            .expect("the editor updates the table");
        assert_eq!(
            committed(&d),
            expected,
            "update table of contents ({name}): editor and command differ"
        );
    }
}

/// The caption the editor's dialog defaults to, plus what each case varies.
fn caption(label: &str, position: CaptionPosition, chapter: Option<CaptionChapter>) -> CaptionSpec {
    CaptionSpec {
        label: label.to_owned(),
        text: ": parity".to_owned(),
        position,
        exclude_label: false,
        number_format: CaptionNumberFormat::Arabic,
        chapter,
    }
}

#[test]
fn captions_and_cross_references_are_one_lowering_for_the_editor_and_agents() {
    let mut d = reference_document();
    let [_, _, wiring, _, layout] = paragraphs(&d)[..] else {
        panic!("five paragraphs");
    };

    // Below "The layout", then above "The wiring": the second renumbers the
    // first (a `SetInlines`), and a chapter-numbered one counts the headings.
    let cases = [
        (layout, caption("Figure", CaptionPosition::Below, None)),
        (wiring, caption("Figure", CaptionPosition::Above, None)),
        (
            layout,
            caption(
                "Table",
                CaptionPosition::Below,
                Some(CaptionChapter {
                    heading_level: 1,
                    separator: CaptionSeparator::Period,
                }),
            ),
        ),
    ];
    for (target, spec) in cases {
        let mut expected_ids = ids(&d);
        let expected = {
            let facts = EditorFacts::with_pages(d.document.definitions(), HashMap::new());
            references::insert_caption(
                &d.document,
                &facts,
                target,
                spec.clone(),
                &mut mint(&mut expected_ids),
            )
            .expect("lowers")
        };
        let chapter = spec.chapter.map_or(0, |chapter| chapter.heading_level);
        let separator = spec
            .chapter
            .map_or("hyphen", |chapter| chapter.separator.wire_name());
        d.insert_caption(
            &target.to_string(),
            &spec.label,
            spec.text.clone(),
            if spec.position == CaptionPosition::Above {
                "above"
            } else {
                "below"
            },
            spec.exclude_label,
            spec.number_format.wire_name(),
            chapter,
            separator,
        )
        .expect("the editor inserts a caption");
        assert_eq!(
            committed(&d),
            expected.operations,
            "caption {} {:?}: editor and command differ",
            spec.label,
            spec.position
        );
    }

    // A stale number, as a document another producer saved arrives with: the
    // first caption's cached `SEQ` result is overwritten in place.
    let first_caption = paragraphs(&d)[2];
    for block in d.document.body_mut() {
        if let BlockNode::Paragraph(paragraph) = block
            && paragraph.id == first_caption
            && let Some(InlineNode::Field(field)) = paragraph
                .inlines
                .iter_mut()
                .find(|inline| matches!(inline, InlineNode::Field(_)))
            && let Some(InlineNode::Run(run)) = field.inlines.first_mut()
        {
            run.text = "9".to_owned();
        }
    }
    let mut expected_ids = ids(&d);
    let expected = {
        let facts = EditorFacts::with_pages(d.document.definitions(), HashMap::new());
        references::update_caption_numbers(&d.document, &facts, &mut mint(&mut expected_ids))
            .expect("one caption is stale")
    };
    d.update_caption_numbers_inner()
        .expect("the editor renumbers");
    assert_eq!(
        committed(&d),
        expected.operations,
        "update caption numbers: editor and command differ"
    );

    // Cross-references to that caption: each kind, with and without a link and
    // the above/below suffix, and a page reference (the editor's one layout
    // query, lent as the target's page).
    let caret = Pos::new(layout, 3);
    for (to, hyperlink, above_below) in [
        (ReferenceTo::EntireCaption, false, true),
        (ReferenceTo::LabelAndNumber, true, false),
        (ReferenceTo::CaptionTextOnly, false, false),
        (ReferenceTo::PageNumber, true, false),
        (ReferenceTo::AboveBelow, false, false),
        (ReferenceTo::ParagraphText, false, true),
    ] {
        let target = paragraphs(&d)[2];
        let mut pages = HashMap::new();
        if let Some(page) = d.page_of(target) {
            pages.insert(target, page.to_string());
        }
        let mut expected_ids = ids(&d);
        let expected = {
            let facts = EditorFacts::with_pages(d.document.definitions(), pages);
            references::insert_cross_reference(
                &d.document,
                &facts,
                caret,
                target,
                to,
                hyperlink,
                above_below,
                &mut mint(&mut expected_ids),
            )
            .expect("lowers")
        };
        d.insert_cross_reference(
            &layout.to_string(),
            caret.offset,
            &target.to_string(),
            to.wire_name(),
            hyperlink,
            above_below,
        )
        .expect("the editor inserts a cross-reference");
        assert_eq!(
            committed(&d),
            expected,
            "cross-reference {}: editor and command differ",
            to.wire_name()
        );
    }
}

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
        // Lane families-b: links, bookmarks, fields, notes, breaks, sections,
        // running content, pictures and object formatting.
        // (`SetExtent`, `SetMediaReference` and `DeleteObject` are not listed:
        // group resizing, picture bullets, Change Picture and group-member
        // deletion build them for their own gestures, which have not moved.)
        // Lane references: tables of contents, captions and cross-references
        // were the editor's last builders of these three.
        "Operation::CreateBookmark {",
        "Operation::InsertField {",
        "Operation::InsertFieldRange {",
        "caption_paragraph(",
        "caption_style(",
        "reference_field(",
        "above_below(",
        "fn toc_blocks(",
        "fn toc_headings(",
        "fn sequence_of(",
        "fn renumbered(",
        "fn paragraph_offsets(",
        "Operation::SetHyperlink {",
        "Operation::RenameBookmark {",
        "Operation::DeleteBookmark {",
        "Operation::InsertNote {",
        "Operation::SpliceSectionBoundary {",
        "Operation::SetSectionGeometry {",
        "Operation::SetSectionLineNumbering {",
        "Operation::SetSectionPageNumbering {",
        "Operation::SetSectionWatermark {",
        "Operation::SetSectionTitlePage {",
        "Operation::SetEvenAndOddHeaders {",
        "Operation::CreateHeaderFooterBody {",
        "Operation::SetImageCrop {",
        "Operation::SetObjectDescr {",
        "Operation::SetShapeFill {",
        "Operation::SetShapeStroke {",
        "fn default_chart_projection(",
        "fn chart_group_for_kind(",
        "InlineNode::Break(Break {",
        "insert_break_op(",
        "section_break_ops(",
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
