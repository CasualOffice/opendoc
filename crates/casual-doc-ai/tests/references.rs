// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §12, lane references: an agent's table of contents, captions and
//! cross-references are the editor's own lowerings, with what they need from
//! layout supplied by a layout pass of the session's state. These guards hold
//! the parts an agent cannot get from the model alone: the page a heading is
//! on, the label that page prints, and the chapter a caption is in.

use std::io::Write;

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat};
use casual_doc_ai::{Profile, Session, SessionConfig};
use casual_doc_model::v1::{BlockNode, InlineNode};
use opendoc_viewer::{ViewerDocument, ViewerOptions};
use serde_json::{Value, json};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A Word package whose body is `body` (paragraph XML) and whose section ends
/// with `section` (extra `w:sectPr` children), with Normal, heading 1 and
/// heading 2 defined.
fn word_file(body: &str, section: &str) -> Vec<u8> {
    let parts = [
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
             <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
             <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
             </Types>"
                .to_owned(),
        ),
        (
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
             </Relationships>"
                .to_owned(),
        ),
        (
            "word/_rels/document.xml.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
             </Relationships>"
                .to_owned(),
        ),
        (
            "word/styles.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <w:styles xmlns:w=\"{W}\">\
                 <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
                 <w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr></w:style>\
                 <w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr></w:style>\
                 </w:styles>"
            ),
        ),
        (
            "word/document.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <w:document xmlns:w=\"{W}\"><w:body>{body}\
                 <w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"/>{section}</w:sectPr>\
                 </w:body></w:document>"
            ),
        ),
    ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, part) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .expect("a part");
        zip.write_all(part.as_bytes()).expect("written");
    }
    zip.finish().expect("a zip").into_inner()
}

fn heading(style: &str, text: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}

fn plain(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

fn open(bytes: &[u8]) -> Session {
    let config = SessionConfig {
        profile: Profile::Edit,
        ..SessionConfig::default()
    };
    Session::open(bytes, "references.docx", config).expect("the fixture opens")
}

fn apply(session: &mut Session, mode: &str, edits: Value) -> Result<Value, casual_doc_ai::AiError> {
    let plan: EditPlan = serde_json::from_value(json!({
        "base_revision": session.revision(),
        "mode": mode,
        "edits": edits,
    }))
    .expect("a well-formed plan");
    plan::apply(session, &plan)
}

/// The id of the body paragraph whose text is `text`.
fn id_of(session: &Session, text: &str) -> String {
    let blocks = read::read_blocks(session, None, None, Some(50_000), ReadFormat::Json)
        .expect("the body reads");
    blocks["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .find(|block| block["text"] == text)
        .and_then(|block| block["id"].as_str())
        .unwrap_or_else(|| panic!("no paragraph reads {text:?}"))
        .to_owned()
}

/// Every body paragraph's text, in order.
fn lines(session: &Session) -> Vec<String> {
    session
        .document()
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(casual_doc_ai::text::paragraph_text(paragraph)),
            _ => None,
        })
        .collect()
}

/// A table of contents an agent inserts names the page each heading is on —
/// read from a layout pass, not invented. The fixture is tall enough that the
/// second chapter is not on page 1, and the expected page is read
/// independently, from the viewer's own outline of the same document.
#[test]
fn an_agent_table_of_contents_lists_the_page_each_heading_is_laid_out_on() {
    let filler: String = (0..80).map(|n| plain(&format!("Body line {n}"))).collect();
    let body = format!(
        "{}{}{filler}{}{filler}",
        plain("Preface"),
        heading("Heading1", "Chapter A"),
        heading("Heading2", "Chapter B"),
    );
    let mut session = open(&word_file(&body, ""));
    let outline = ViewerDocument::from_document(
        session.document().clone(),
        Default::default(),
        &ViewerOptions::default(),
    )
    .outline();
    let expected: Vec<String> = outline
        .iter()
        .map(|entry| format!("{}\t{}", entry.text, entry.page.expect("laid out") + 1))
        .collect();
    assert!(
        expected.iter().any(|line| !line.ends_with("\t1")),
        "the fixture must put a heading past page 1, or the page column proves nothing: \
         {expected:?}"
    );

    let preface = id_of(&session, "Preface");
    apply(
        &mut session,
        "direct",
        json!([{ "op": "insert_table_of_contents", "before": { "node": preface } }]),
    )
    .expect("the contents inserts");
    assert_eq!(
        &lines(&session)[..2],
        &expected[..],
        "one entry per heading: its text, a tab, and the page the layout puts it on"
    );
}

/// The headings come from the plan's own state: a heading the same plan made
/// is listed, so a plan that structures a document and then adds its contents
/// gets one that agrees with it.
#[test]
fn a_table_of_contents_lists_a_heading_made_earlier_in_the_same_plan() {
    let body = format!("{}{}", plain("Preface"), plain("Scope"));
    let mut session = open(&word_file(&body, ""));
    let preface = id_of(&session, "Preface");
    let scope = id_of(&session, "Scope");
    apply(
        &mut session,
        "direct",
        json!([
            { "op": "apply_paragraph_style", "target": { "node": scope }, "style": "heading 1" },
            { "op": "insert_table_of_contents", "before": { "node": preface } },
        ]),
    )
    .expect("the plan applies");
    assert_eq!(lines(&session)[0], "Scope\t1");
}

/// A page cross-reference prints the page's LABEL, which is what Word's
/// `PAGEREF` shows: a section numbered in lower-case roman from 5 prints `v`
/// on its first page, where the page's index would say `1`.
#[test]
fn a_page_cross_reference_prints_the_label_the_page_prints() {
    let body = format!("{}{}", plain("See the terms on page "), plain("The terms"));
    let mut session = open(&word_file(
        &body,
        "<w:pgNumType w:fmt=\"lowerRoman\" w:start=\"5\"/>",
    ));
    let at = id_of(&session, "See the terms on page ");
    let target = id_of(&session, "The terms");
    apply(
        &mut session,
        "direct",
        json!([{ "op": "insert_cross_reference", "at": { "node": at, "side": "after" },
                 "target": target, "show": "page_number" }]),
    )
    .expect("the reference inserts");
    let shown = session
        .document()
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.inlines.iter().find_map(|inline| match inline {
                    InlineNode::Field(field) if field.instruction.contains("PAGEREF") => {
                        Some(casual_doc_layout::flow::node_plain_text(&field.inlines))
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .expect("a PAGEREF field");
    assert_eq!(shown, "v", "the label the page prints, not its index");
}

/// A chapter-numbered caption counts the headings before it, which the agent
/// layer reads through the same style cascade the editor does. And the
/// command is direct only: in suggest mode it is refused with the registry's
/// published reason.
#[test]
fn a_chapter_numbered_caption_counts_the_headings_before_it() {
    let body = format!(
        "{}{}{}{}",
        heading("Heading1", "Scope"),
        plain("First."),
        heading("Heading1", "Method"),
        plain("The wiring diagram"),
    );
    let mut session = open(&word_file(&body, ""));
    let target = id_of(&session, "The wiring diagram");
    let edit = json!({ "op": "insert_caption", "target": target, "label": "Figure",
                       "text": ": Wiring", "chapter_level": 1 });
    let refused = apply(&mut session, "suggest", json!([edit.clone()])).expect_err("not trackable");
    assert!(
        format!("{refused:?}").contains("not-trackable"),
        "suggest mode is refused with the published reason: {refused:?}"
    );
    apply(&mut session, "direct", json!([edit])).expect("the caption inserts");
    let all = lines(&session);
    assert_eq!(
        all.last().map(String::as_str),
        Some("Figure 2-1: Wiring"),
        "the second chapter's first figure: {all:?}"
    );
}

/// A one-entry table of contents begins and ends in one paragraph, so it reopens
/// from DOCX as an INLINE `TOC` field (`docs/128` §2c: a field that fits in a
/// paragraph is inline). The update command used to look for ranges only and said
/// "no table of contents to update"; it now finds the inline one and rebuilds it as
/// a range, as Word does when an updated `TOC` result no longer fits (`docs/181` §12).
#[test]
fn a_one_entry_table_of_contents_still_updates_after_a_docx_round_trip() {
    let body = format!("{}{}", plain("Preface"), heading("Heading1", "Scope"));
    let mut session = open(&word_file(&body, ""));
    let preface = id_of(&session, "Preface");
    apply(
        &mut session,
        "direct",
        json!([{ "op": "insert_table_of_contents", "before": { "node": preface } }]),
    )
    .expect("the contents inserts");
    let saved = session
        .export(casual_doc_ai::ExportFormat::Docx, &[])
        .expect("the save loses nothing")
        .bytes;
    let mut reopened = open(&saved);
    let toc_ranges = |session: &Session| {
        session
            .document()
            .definitions()
            .field_ranges
            .iter()
            .filter(|(_, range)| range.kind == casual_doc_model::v1::FieldKind::Toc)
            .count()
    };
    assert_eq!(
        toc_ranges(&reopened),
        0,
        "it reopens inline, per docs/128 §2c"
    );
    apply(
        &mut reopened,
        "direct",
        json!([{ "op": "update_table_of_contents", "mode": "entire" }]),
    )
    .expect("the reopened contents updates");
    assert_eq!(lines(&reopened)[0], "Scope\t1");
    assert_eq!(toc_ranges(&reopened), 1, "the updated contents is a range");
}
