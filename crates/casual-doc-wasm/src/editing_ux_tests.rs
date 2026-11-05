// SPDX-License-Identifier: Apache-2.0

//! Editing-feel guards for the batch `docs/109` UX-057 to UX-059: Tab types a
//! tab, Undo and Redo hand back the selection they restored, and a range delete
//! removes the tables it covers. Each test names the mutation that turns it red.

use std::collections::BTreeMap;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};

use crate::{WasmDocument, open_document, write_document};

/// The top-level body paragraphs, as `(id, text)`.
fn paragraphs(d: &WasmDocument) -> Vec<(NodeId, String)> {
    d.document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some((paragraph.id, d.paragraph_text(paragraph.id))),
            _ => None,
        })
        .collect()
}

fn first(d: &WasmDocument) -> (String, NodeId) {
    let (id, _) = paragraphs(d)[0].clone();
    (id.to_string(), id)
}

fn tables(d: &WasmDocument) -> usize {
    d.document
        .body()
        .iter()
        .filter(|block| matches!(block, BlockNode::Table(_)))
        .count()
}

fn inlines_of(d: &WasmDocument, node: NodeId) -> Vec<InlineNode> {
    d.document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                Some(paragraph.inlines.clone())
            }
            _ => None,
        })
        .expect("the paragraph")
}

/// The anchor and focus an edit result hands back.
fn ends(result: &crate::EditResult) -> ((String, u32), (String, u32)) {
    (
        (result.anchor_node(), result.anchor_offset()),
        (result.node(), result.offset()),
    )
}

/// A body table block of `rows` x `cols` empty cells, made by the editor's own
/// Insert Table so its shape is exactly what a user gets.
fn table_block(rows: u32, cols: u32) -> BlockNode {
    let mut seed = open_document(b"seed").expect("open");
    let (node, _) = first(&seed);
    seed.insert_table_inner(&node, rows, cols)
        .expect("insert table");
    seed.document
        .body()
        .iter()
        .find(|block| matches!(block, BlockNode::Table(_)))
        .cloned()
        .expect("a table")
}

/// A document of `blocks`, where `Some(text)` is a paragraph and `None` is a 2x2
/// table, written and reopened so it is a document the editor really opened.
fn document_of(blocks: &[Option<&str>]) -> WasmDocument {
    let id = |n: u64| NodeId::from_parts(n, 0x5eed).expect("id");
    let body = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| match block {
            Some(text) => BlockNode::Paragraph(Paragraph {
                id: id(10 + 2 * index as u64),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: id(11 + 2 * index as u64),
                    properties: RunProperties::default().into(),
                    text: (*text).to_owned(),
                })],
            }),
            None => table_block(2, 2),
        })
        .collect();
    let document = Document::new(id(1), body, Definitions::default()).expect("valid");
    let bytes = write_document(&document, &BTreeMap::new()).expect("write");
    open_document(&bytes).expect("reopen")
}

/// Every paragraph in document order (cells included), as `(id, length)`.
fn ordered(d: &WasmDocument) -> Vec<(String, u32)> {
    d.ordered_paragraphs()
        .into_iter()
        .map(|(id, len)| (id.to_string(), len))
        .collect()
}

// --- UX-057: Tab types a tab ---------------------------------------------------

/// Tab in body text is a real `InlineNode::Tab` at the caret, the caret lands
/// after it, and one Undo removes it. Mutation: lower `insert_tab` to an
/// `InsertText` of "\t" and the inline check goes red (a Run holds the tab).
#[test]
fn insert_tab_puts_a_tab_inline_at_the_caret_and_one_undo_removes_it() {
    let mut d = open_document(b"alpha").expect("open");
    let (node, id) = first(&d);
    let result = d.insert_tab_inner(&node, 5, &node, 5).expect("tab");
    assert_eq!((result.node(), result.offset()), (node.clone(), 6));
    let inlines = inlines_of(&d, id);
    assert!(
        matches!(inlines.last(), Some(InlineNode::Tab(_))),
        "{inlines:?}"
    );
    assert!(inlines.iter().all(|inline| match inline {
        InlineNode::Run(run) => !run.text.contains('\t'),
        _ => true,
    }));
    d.undo_inner().expect("undo");
    assert_eq!(d.paragraph_text(id), "alpha");
    assert!(
        !inlines_of(&d, id)
            .iter()
            .any(|inline| matches!(inline, InlineNode::Tab(_)))
    );
}

/// Over a selection the tab REPLACES it, as one undoable step.
#[test]
fn insert_tab_replaces_the_selection_in_one_step() {
    let mut d = open_document(b"alpha beta").expect("open");
    let (node, id) = first(&d);
    d.insert_tab_inner(&node, 5, &node, 6)
        .expect("tab over the space");
    assert_eq!(d.paragraph_text(id), "alpha\tbeta");
    d.undo_inner().expect("undo");
    assert_eq!(
        d.paragraph_text(id),
        "alpha beta",
        "one undo restores it all"
    );
}

/// A pasted or typed "\t" is a Tab inline, never a U+0009 inside a run, and it
/// saves as `<w:tab/>`. Mutation: drop the `'\t'` split in `plain_text_ops` and
/// the run check goes red with "a\tb" in one run.
#[test]
fn plain_text_with_a_tab_lowers_to_run_tab_run_and_saves_as_w_tab() {
    let mut d = open_document(b"x").expect("open");
    let (node, id) = first(&d);
    d.insert_plain_text(&node, 0, &node, 1, "a\tb".to_owned())
        .expect("paste");
    let shape: Vec<String> = inlines_of(&d, id)
        .iter()
        .map(|inline| match inline {
            InlineNode::Run(run) => format!("run:{}", run.text),
            InlineNode::Tab(_) => "tab".to_owned(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(shape, ["run:a", "tab", "run:b"]);
    let bytes = d.export_docx().expect("save");
    let mut package =
        casual_doc_ooxml::DocxPackage::open(&bytes, crate::viewer_limits()).expect("package");
    let xml =
        String::from_utf8(package.read_part("word/document.xml").expect("part")).expect("utf-8");
    assert!(xml.contains("<w:tab/>"), "{xml}");
    assert!(!xml.contains('\t'), "no literal tab: {xml}");
    // And the paste undoes as one step, tab included: the tab's inverse is a
    // `RemoveInlineObject`, which has to be able to find a tab
    // (`is_removable_inline_node`).
    d.undo_inner().expect("undo the paste");
    assert_eq!(d.paragraph_text(id), "x");
}

// --- UX-058: Undo and Redo restore the selection --------------------------------

/// Undoing a format change re-selects the formatted range; it used to land the
/// caret at the start of the paragraph. Redo selects it again. Mutation: make
/// `EditResult::select` a no-op and this goes red with a collapsed caret at 0.
#[test]
fn undoing_a_format_reselects_the_formatted_range() {
    let mut d = open_document(b"one two three").expect("open");
    let (node, _) = first(&d);
    d.format_selection(&node, 8, &node, 13, Some(true), None, None, None)
        .expect("bold 'three'");
    let undone = d.undo_inner().expect("undo");
    assert_eq!(ends(&undone), ((node.clone(), 8), (node.clone(), 13)));
    let redone = d.redo_inner().expect("redo");
    assert_eq!(ends(&redone), ((node.clone(), 8), (node.clone(), 13)));
}

/// Undoing a delete selects the text it brought back; redoing it collapses at
/// the deletion point.
#[test]
fn undoing_a_delete_selects_the_restored_text() {
    let mut d = open_document(b"one two three").expect("open");
    let (node, id) = first(&d);
    d.delete_selection(&node, 4, &node, 13).expect("delete");
    assert_eq!(d.paragraph_text(id), "one ");
    let undone = d.undo_inner().expect("undo");
    assert_eq!(ends(&undone), ((node.clone(), 4), (node.clone(), 13)));
    let redone = d.redo_inner().expect("redo");
    assert_eq!(ends(&redone), ((node.clone(), 4), (node.clone(), 4)));
}

/// A delete across paragraphs restores, and re-selects, both halves.
#[test]
fn undoing_a_cross_paragraph_delete_selects_across_both_paragraphs() {
    let mut d = open_document(b"first line\nsecond line").expect("open");
    let paras = paragraphs(&d);
    let (a, b) = (paras[0].0.to_string(), paras[1].0.to_string());
    d.delete_selection(&a, 6, &b, 6).expect("delete");
    let undone = d.undo_inner().expect("undo");
    assert_eq!(ends(&undone), ((a, 6), (b, 6)));
}

/// Undoing typing collapses where the text was typed.
#[test]
fn undoing_typing_collapses_where_it_was_typed() {
    let mut d = open_document(b"one").expect("open");
    let (node, _) = first(&d);
    d.insert_text(&node, 3, " two".to_owned()).expect("type");
    let undone = d.undo_inner().expect("undo");
    assert_eq!(ends(&undone), ((node.clone(), 3), (node.clone(), 3)));
}

// --- UX-059: a range delete removes the tables it covers ------------------------

/// [table, "tail"]: select from the first cell's start to the end of "tail" and
/// delete — Word's Ctrl+A, Delete in a document that opens with a table. The
/// table goes, one empty paragraph is left, and Undo puts it all back.
/// Mutation: skip the `delete_table` branch and the table count stays 1.
#[test]
fn a_select_all_delete_removes_a_leading_table() {
    let mut d = document_of(&[None, Some("tail")]);
    assert_eq!(tables(&d), 1);
    let order = ordered(&d);
    let (start, end) = (order[0].clone(), order[order.len() - 1].clone());
    let result = d
        .delete_selection(&start.0, 0, &end.0, end.1)
        .expect("delete");
    assert_eq!(tables(&d), 0, "the covered table is removed");
    assert_eq!(d.document.body().len(), 1, "one paragraph is left");
    assert_eq!(paragraphs(&d)[0].1, "", "and it is empty");
    assert_eq!(
        result.node(),
        paragraphs(&d)[0].0.to_string(),
        "the caret is in it"
    );
    // Undo restores the table and the text.
    d.undo_inner().expect("undo");
    assert_eq!(tables(&d), 1);
    assert_eq!(
        paragraphs(&d).last().map(|p| p.1.clone()),
        Some("tail".to_owned())
    );
    // Redo removes it again, and typing into what is left is an ordinary edit
    // (`docs/174` TA-02 measured "That edit isn't supported" here).
    d.redo_inner().expect("redo");
    assert_eq!(tables(&d), 0);
    let order = ordered(&d);
    assert_eq!(order.len(), 1);
    let only = order[0].0.clone();
    d.insert_text(&only, 0, "fresh".to_owned()).expect("type");
    assert_eq!(paragraphs(&d)[0].1, "fresh");
}

/// ["a", table, "b"]: a delete from "a" to the end of "b" removes the table and
/// leaves one empty paragraph.
#[test]
fn a_delete_across_a_table_removes_it_and_joins_the_paragraphs_around_it() {
    let mut d = document_of(&[Some("a"), None, Some("b")]);
    let paras = paragraphs(&d);
    let (a, b) = (paras[0].0.to_string(), paras[1].0.to_string());
    d.delete_selection(&a, 0, &b, 1).expect("delete");
    assert_eq!(tables(&d), 0);
    assert_eq!(d.document.body().len(), 1);
    assert_eq!(paragraphs(&d)[0].1, "");
    d.undo_inner().expect("undo");
    assert_eq!(tables(&d), 1);
    assert_eq!(d.document.body().len(), 3);
}

/// Typing over a selection that covers a table replaces it with the text, in the
/// paragraph that survives — the same delete serves type-over and paste.
#[test]
fn typing_over_a_selection_that_covers_a_table_lands_in_the_surviving_paragraph() {
    let mut d = document_of(&[Some("a"), None, Some("b")]);
    let paras = paragraphs(&d);
    let (a, b) = (paras[0].0.to_string(), paras[1].0.to_string());
    d.replace_selection(&a, 0, &b, 1, "X".to_owned())
        .expect("type over");
    assert_eq!(tables(&d), 0);
    assert_eq!(paragraphs(&d).len(), 1);
    assert_eq!(paragraphs(&d)[0].1, "X");
}

/// ["a", table, "b"] with the selection ending INSIDE the table: the table is
/// kept and only the covered cells are cleared, which is Word's answer too.
#[test]
fn a_delete_that_ends_inside_a_table_keeps_it_and_clears_the_covered_cells() {
    let mut d = document_of(&[Some("a"), None, Some("b")]);
    let order = ordered(&d);
    // order: a, cell(0,0), cell(0,1), cell(1,0), cell(1,1), b
    let (a, cell) = (order[0].0.clone(), order[2].0.clone());
    d.insert_text(&cell, 0, "kept?".to_owned())
        .expect("type in a cell");
    d.delete_selection(&a, 0, &cell, 2).expect("delete");
    assert_eq!(tables(&d), 1, "a partly covered table stays");
    let after = ordered(&d);
    let cell_len = after
        .iter()
        .find(|(id, _)| *id == cell)
        .map(|(_, len)| *len);
    assert_eq!(cell_len, Some(3), "only the covered head of the cell went");
}
