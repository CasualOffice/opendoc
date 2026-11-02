// SPDX-License-Identifier: Apache-2.0

//! The caret and story contract (`docs/109` UX-057, UX-058; audit CRITIC-01,
//! CANVAS-01, CANVAS-02).
//!
//! Each guard asserts the GUARANTEE a reader relies on — "the footer did not
//! change", "the item is drawn one level in, with its bullet" — on the
//! committed `sample.docx`, whose three sections each carry a header and a
//! footer and whose bullets take their list from the *List Bullet* style.

use casual_doc_edit::{Pos, paragraph_properties};
use casual_doc_model::NodeId;
use casual_doc_model::v1::NumberingRef;

use crate::list_levels::{ListItem, level_change};
use crate::story::BODY_STORY;
use crate::{WasmDocument, open_document};

const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");

fn sample() -> WasmDocument {
    open_document(SAMPLE_DOCX).expect("open sample.docx")
}

/// The paragraph whose whole text is `text`.
fn para(d: &WasmDocument, text: &str) -> NodeId {
    d.ordered_paragraphs()
        .into_iter()
        .find(|(id, _)| d.paragraph_text(*id) == text)
        .unwrap_or_else(|| panic!("sample.docx has a paragraph {text:?}"))
        .0
}

/// Every paragraph text of every story other than the body, in order.
fn other_stories(d: &WasmDocument) -> Vec<String> {
    d.ordered_paragraphs_by_surface()
        .into_iter()
        .filter(|(_, _, story)| *story != BODY_STORY)
        .map(|(id, _, _)| d.paragraph_text(id))
        .collect()
}

/// The body's paragraph ids, in order.
fn body(d: &WasmDocument) -> Vec<NodeId> {
    d.ordered_paragraphs_by_surface()
        .into_iter()
        .filter(|(_, _, story)| *story == BODY_STORY)
        .map(|(id, _, _)| id)
        .collect()
}

/// The page-local x (twips) where `node`'s text starts.
fn text_x(d: &WasmDocument, node: NodeId) -> i32 {
    d.caret_rect(&node.to_string(), 0)[1]
}

#[test]
fn document_end_is_the_end_of_the_body_and_typing_there_leaves_every_footer_alone() {
    let mut d = sample();
    assert!(
        !other_stories(&d).is_empty(),
        "precondition: sample.docx carries headers and footers"
    );
    let footers_before = other_stories(&d);
    let last_body = *body(&d).last().expect("a body");

    let end = d.last_position();
    assert_eq!(
        end.node,
        last_body.to_string(),
        "Ctrl+End lands in the last BODY paragraph, not a footer"
    );
    assert_eq!(d.story_kind(&end.node), "body");
    let before = d.paragraph_text(last_body);
    d.insert_text(&end.node, end.offset, "ZZZ".into())
        .expect("type at the document end");

    assert_eq!(d.paragraph_text(last_body), format!("{before}ZZZ"));
    assert_eq!(
        other_stories(&d),
        footers_before,
        "no header, footer or note changed"
    );
}

#[test]
fn document_start_is_the_start_of_the_body() {
    let d = sample();
    let start = d.first_position();
    assert_eq!(start.node, body(&d)[0].to_string());
    assert_eq!(start.offset, 0);
}

#[test]
fn an_open_story_has_its_own_edges_and_kind() {
    let d = sample();
    let stories = d.ordered_paragraphs_by_surface();
    let (footer_node, footer_story) = stories
        .iter()
        .find(|(id, _, story)| *story != BODY_STORY && d.story_kind(&id.to_string()) == "footer")
        .map(|(id, _, story)| (*id, *story))
        .expect("a footer paragraph");
    let last_in_footer = stories
        .iter()
        .rfind(|(_, _, story)| *story == footer_story)
        .expect("the footer has a paragraph");

    let edge = d.story_edge(&footer_node.to_string(), true);
    assert_eq!(edge.node, last_in_footer.0.to_string());
    assert_eq!(edge.offset, last_in_footer.1);
    assert!(
        stories
            .iter()
            .any(|(id, _, _)| d.story_kind(&id.to_string()) == "header")
    );
    assert_eq!(d.story_kind(&body(&d)[0].to_string()), "body");
}

#[test]
fn paragraph_steps_never_leave_the_body() {
    let d = sample();
    let last = *body(&d).last().expect("a body");
    let len = u32::try_from(d.paragraph_text(last).len()).expect("short");
    let (node, _) = d.moved_caret(last, 0, "paragraphDown", None);
    assert_eq!(
        node, last,
        "Ctrl+Down from the last body paragraph stays put"
    );
    let (node, offset) = d.moved_caret(last, len, "right", None);
    assert_eq!((node, offset), (last, len));
}

#[test]
fn a_deletion_never_spans_two_stories() {
    let d = sample();
    let first = body(&d)[0];
    let footer = d
        .ordered_paragraphs_by_surface()
        .into_iter()
        .find(|(_, _, story)| *story != BODY_STORY)
        .expect("another story")
        .0;
    let refused = d.selection_delete_ops_ordered(Pos::new(first, 0), Pos::new(footer, 0));
    assert!(refused.is_err(), "body → footer is not one deletable range");
}

#[test]
fn tab_on_a_style_numbered_bullet_moves_it_one_level_in_and_keeps_its_bullet() {
    let mut d = sample();
    let second = para(&d, "Second bullet");
    let nested = para(&d, "Nested bullet");
    let s = second.to_string();
    assert_eq!(
        d.list_style_at(&s),
        "bullet",
        "a List Bullet item is a list item"
    );
    let home = text_x(&d, second);
    let target = text_x(&d, nested);
    assert!(target > home, "precondition: the nested item is further in");

    d.adjust_list_level(&s, 0, &s, 0, 1).expect("Tab demotes");
    assert_eq!(
        text_x(&d, second),
        target,
        "Tab lines the item up with the nested bullet, one step RIGHT"
    );
    assert_eq!(d.list_style_at(&s), "bullet", "it keeps its bullet");

    d.adjust_list_level(&s, 0, &s, 0, -1)
        .expect("Shift+Tab promotes");
    assert_eq!(text_x(&d, second), home, "Shift+Tab returns it exactly");
    assert_eq!(
        paragraph_properties(&d.document, second).and_then(|p| p.indentation),
        None,
        "and leaves no direct indent behind"
    );
}

#[test]
fn shift_tab_at_the_outermost_level_never_moves_a_bullet_into_the_margin() {
    let d = sample();
    let second = para(&d, "Second bullet");
    let direct = paragraph_properties(&d.document, second).expect("props");
    let effective = casual_doc_layout::cascade::StyleCascade::new(d.document.definitions())
        .resolve_paragraph(&direct);
    let item = ListItem {
        inherited: effective.indentation,
        direct,
        effective,
    };
    let resolver = d.document.definitions().numbering_resolver();
    let numbering = item.effective.numbering.expect("style-numbered");
    let level_indent = |level: u8| {
        resolver
            .level(NumberingRef { level, ..numbering })
            .map(|l| l.paragraph_properties.as_ref().and_then(|p| p.indentation))
    };
    assert!(
        level_change(&item, &level_indent, -1).is_none(),
        "an item already at its style's position does not move left"
    );
}

#[test]
fn increase_indent_moves_a_style_indented_paragraph_right() {
    let mut d = sample();
    let second = para(&d, "Second bullet");
    let s = second.to_string();
    let home = text_x(&d, second);
    d.adjust_indent(&s, 0, &s, 0, 360).expect("indent");
    assert_eq!(text_x(&d, second), home + 360);
}

#[test]
fn bullets_off_ends_a_style_supplied_list() {
    let mut d = sample();
    let second = para(&d, "Second bullet");
    let s = second.to_string();
    d.toggle_list(&s, 0, &s, 0, "bullet")
        .expect("Bullets toggles off");
    assert_eq!(d.list_style_at(&s), "", "the item left the list");
    assert!(
        paragraph_properties(&d.document, second).is_some_and(|p| p.numbering_none),
        "the style's list is cancelled with numId 0"
    );
    d.toggle_list(&s, 0, &s, 0, "bullet")
        .expect("Bullets toggles on");
    assert_eq!(d.list_style_at(&s), "bullet");
}

#[test]
fn a_multilevel_list_takes_its_level_indent_with_no_direct_indent() {
    let mut d = sample();
    let plain = body(&d)
        .into_iter()
        .find(|id| d.list_style_at(&id.to_string()).is_empty() && !d.paragraph_text(*id).is_empty())
        .expect("a plain paragraph");
    let p = plain.to_string();
    d.toggle_list(&p, 0, &p, 0, "numbered").expect("number it");
    let level0 = text_x(&d, plain);
    d.adjust_list_level(&p, 0, &p, 0, 1).expect("demote");
    assert!(text_x(&d, plain) > level0, "one level in is further right");
    assert_eq!(d.list_level_at(&p), 1);
    d.adjust_list_level(&p, 0, &p, 0, -1).expect("promote");
    assert_eq!(text_x(&d, plain), level0);
}

/// A heading and a 1×1 table, the shape `htmlToStructured` emits for foreign
/// HTML ending in a table (audit CRITIC-03).
const HEADING_THEN_TABLE: &str = r#"{"blocks":[
    {"kind":"paragraph","runs":[{"text":"Pasted heading"}]},
    {"kind":"table","rows":[[{"blocks":[{"kind":"paragraph","runs":[{"text":"cell"}]}]}]]}
]}"#;

/// Whether `node` is a paragraph inside a table.
fn in_table(d: &WasmDocument, node: &str) -> bool {
    d.in_table(node)
}

#[test]
fn a_paste_ending_in_a_table_leaves_the_caret_after_the_table_at_the_body_end() {
    let mut d = sample();
    let last = *body(&d).last().expect("a body");
    let l = last.to_string();
    let len = d.paragraph_length(&l);
    let landed = d
        .paste_external_structured(&l, len, &l, len, HEADING_THEN_TABLE.into())
        .expect("structured paste");
    let (node, offset) = (landed.node.clone(), landed.offset);
    assert!(
        !in_table(&d, &node),
        "the caret is not stranded in the table"
    );
    d.insert_text(&node, offset, "@@END".into())
        .expect("type after the paste");
    assert_eq!(
        d.paragraph_text(para(&d, "@@END")),
        "@@END",
        "typing lands in its own paragraph after the table"
    );
    assert_eq!(
        d.paragraph_text(para(&d, "Pasted heading")),
        "Pasted heading",
        "and not in front of the pasted heading"
    );
}

#[test]
fn a_paste_that_splits_a_paragraph_leaves_the_caret_on_the_tail() {
    let mut d = sample();
    let host = d
        .document
        .body()
        .iter()
        .find_map(|block| match block {
            casual_doc_model::v1::BlockNode::Paragraph(p)
                if d.paragraph_text(p.id).len() > 4 && d.paragraph_text(p.id).is_ascii() =>
            {
                Some(p.id)
            }
            _ => None,
        })
        .expect("a top-level body paragraph with text");
    let h = host.to_string();
    let tail_text = d.paragraph_text(host)[2..].to_owned();
    let landed = d
        .paste_external_structured(&h, 2, &h, 2, HEADING_THEN_TABLE.into())
        .expect("structured paste");
    assert_eq!(landed.offset, 0);
    let tail = landed.node.clone();
    assert_eq!(
        d.paragraph_text(tail.parse().expect("node id")),
        tail_text,
        "the caret is at the start of the split-off tail, after the table"
    );
}
