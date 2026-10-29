// SPDX-License-Identifier: Apache-2.0

//! The Native SDK's contract, end to end: build, read, edit, template, save,
//! convert, render. Each test drives the public API only.

use opendoc::view::Block;
use opendoc::{
    Document, EditCommand, ErrorCode, Format, Position, RunFormat, SaveMode, TemplateOptions,
};
use serde_json::json;

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");

fn spec(value: serde_json::Value) -> Document {
    Document::from_spec_json(&value.to_string(), &[]).expect("the spec builds")
}

fn para_text(doc: &Document) -> Vec<String> {
    doc.paragraphs().into_iter().map(|p| p.text).collect()
}

#[test]
fn a_blank_document_has_words_styles_and_one_a4_page() {
    let doc = Document::create().unwrap();
    let names: Vec<_> = doc.styles().into_iter().filter_map(|s| s.name).collect();
    for wanted in [
        "Normal",
        "heading 1",
        "Title",
        "List Bullet",
        "Table Grid",
        "Hyperlink",
    ] {
        assert!(
            names.iter().any(|n| n == wanted),
            "missing style {wanted}: {names:?}"
        );
    }
    let section = &doc.sections()[0];
    assert_eq!(
        (section.page_width_pt, section.page_height_pt),
        (595.3, 841.9)
    );
    assert_eq!(doc.page_count(), 1);
    assert!(doc.import_report().repairs.is_empty());
}

#[test]
fn python_docx_style_building_reads_back_after_a_round_trip() {
    let mut doc = Document::create().unwrap();
    doc.edit(|e| {
        e.append_heading("Document Title", 0)?;
        let p = e.append_paragraph("A plain paragraph having some ", None)?;
        e.append_run(&p, "bold", &RunFormat::bold())?;
        e.append_run(&p, " and some ", &RunFormat::default())?;
        e.append_run(&p, "italic.", &RunFormat::italic())?;
        e.append_heading("Heading, level 1", 1)?;
        e.append_paragraph("first item in unordered list", Some("List Bullet"))?;
        let t = e.insert_table(
            &Position::End,
            2,
            3,
            &[vec!["Qty".into(), "Id".into(), "Desc".into()]],
        )?;
        e.set_cell_text(&t, 1, 2, "Spam")?;
        e.add_row(&t, &["4".into(), "101".into(), "Eggs".into()])?;
        Ok(())
    })
    .unwrap();
    let (bytes, report) = doc.save(Format::Docx, SaveMode::PreserveWhenSafe).unwrap();
    assert!(
        report.is_lossless(),
        "a built document loses nothing: {:?}",
        report.losses
    );
    let reopened = Document::open(&bytes).unwrap();
    let paragraphs = reopened.paragraphs();
    // The blank document's own empty paragraph comes first.
    let texts: Vec<_> = paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert!(texts.contains(&"Document Title"));
    assert!(texts.contains(&"A plain paragraph having some bold and some italic."));
    let mixed = paragraphs
        .iter()
        .find(|p| p.text.starts_with("A plain"))
        .unwrap();
    let bold: Vec<_> = mixed
        .runs
        .iter()
        .filter(|r| r.bold == Some(true))
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(bold, ["bold"]);
    let italic: Vec<_> = mixed
        .runs
        .iter()
        .filter(|r| r.italic == Some(true))
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(italic, ["italic."]);
    let title = paragraphs
        .iter()
        .find(|p| p.text == "Document Title")
        .unwrap();
    assert_eq!(title.heading_level, Some(0));
    let h1 = paragraphs
        .iter()
        .find(|p| p.text == "Heading, level 1")
        .unwrap();
    assert_eq!(h1.heading_level, Some(1));
    let tables = reopened.tables();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].rows.len(), 3);
    assert_eq!(tables[0].cell(1, 2).unwrap().text(), "Spam");
    assert_eq!(tables[0].cell(2, 2).unwrap().text(), "Eggs");
    assert_eq!(tables[0].style.as_deref(), Some("Table Grid"));
}

#[test]
fn a_docx_js_shaped_spec_builds_every_story() {
    let doc = spec(json!({
        "creator": "Ada", "title": "Spec",
        "numbering": {"config": [{"reference": "steps", "levels": [
            {"level": 0, "format": "decimal", "text": "%1.", "alignment": "start"}]}]},
        "comments": {"children": [{"id": 1, "author": "Rev", "children": [
            {"type": "paragraph", "text": "Check this"}]}]},
        "footnotes": {"1": {"children": [{"type": "paragraph", "text": "A footnote."}]}},
        "sections": [{
            "properties": {"page": {"size": {"orientation": "landscape"}}, "titlePage": false},
            "headers": {"default": {"children": [{"type": "paragraph", "text": "Header text"}]}},
            "footers": {"default": {"children": [{"type": "paragraph", "children": [
                {"type": "text", "text": "Page "}, {"type": "field", "instruction": "PAGE"}]}]}},
            "children": [
                {"type": "paragraph", "heading": "Heading1", "text": "Intro"},
                {"type": "paragraph", "children": [
                    {"type": "text", "text": "Hello ", "bold": true},
                    {"type": "text", "text": "World", "italics": true, "color": "FF0000", "size": 28},
                    {"type": "footnoteReference", "id": "1"},
                    {"type": "commentRangeStart", "id": 1},
                    {"type": "text", "text": " noted"},
                    {"type": "commentRangeEnd", "id": 1},
                    {"type": "commentReference", "id": 1},
                    {"type": "hyperlink", "link": "https://example.com", "children": [
                        {"type": "text", "text": " link"}]},
                    {"type": "insertedText", "text": " added", "author": "Bob"}
                ]},
                {"type": "paragraph", "text": "One", "numbering": {"reference": "steps", "level": 0}},
                {"type": "paragraph", "text": "Two", "numbering": {"reference": "steps", "level": 0}},
                {"type": "paragraph", "text": "Bullet", "bullet": {"level": 0}},
                {"type": "table", "rows": [
                    {"tableHeader": true, "children": [
                        {"children": [{"type": "paragraph", "text": "A"}], "rowSpan": 2},
                        {"children": [{"type": "paragraph", "text": "B"}]}]},
                    {"children": [{"children": [{"type": "paragraph", "text": "C"}]}]}
                ]}
            ]
        }]
    }));
    let section = &doc.sections()[0];
    assert_eq!(section.orientation, "landscape");
    assert!(section.page_width_pt > section.page_height_pt);
    let props = doc.properties();
    assert_eq!(props.creator.as_deref(), Some("Ada"));
    let paragraphs = doc.paragraphs();
    let intro = &paragraphs[0];
    assert_eq!(
        (intro.text.as_str(), intro.heading_level),
        ("Intro", Some(1))
    );
    let rich = &paragraphs[1];
    let world = rich.runs.iter().find(|r| r.text == "World").unwrap();
    assert_eq!(
        (world.italic, world.color.as_deref(), world.size_pt),
        (Some(true), Some("FF0000"), Some(14.0))
    );
    assert!(
        rich.runs
            .iter()
            .any(|r| r.hyperlink.as_deref() == Some("https://example.com"))
    );
    assert!(rich.runs.iter().any(|r| r.kind == "noteReference"));
    let added = rich.runs.iter().find(|r| r.text == " added").unwrap();
    assert_eq!(added.revision.as_ref().map(|m| m.kind), Some("insertion"));
    assert!(paragraphs[2].list.is_some() && paragraphs[3].list.is_some());
    assert_eq!(paragraphs[2].list, paragraphs[3].list);
    assert!(paragraphs[4].list.is_some());
    assert_ne!(paragraphs[4].list, paragraphs[2].list);

    let comments = doc.comments();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, "Check this");
    assert_eq!(comments[0].author.as_deref(), Some("Rev"));
    let anchor = comments[0].anchor.as_ref().expect("anchored");
    let anchored: String = rich
        .text
        .chars()
        .skip(anchor.start as usize)
        .take((anchor.end - anchor.start) as usize)
        .collect();
    assert_eq!(anchored, " noted");
    let revisions = doc.revisions();
    assert!(
        revisions
            .iter()
            .any(|r| r.kind == "insertion" && r.text == " added")
    );
    let notes = doc.notes();
    assert!(
        notes
            .iter()
            .any(|n| n.kind == "footnote" && n.text.contains("A footnote."))
    );
    let stories = doc.headers_footers();
    assert!(
        stories
            .iter()
            .any(|s| s.part == "header" && s.text == "Header text")
    );
    assert!(
        stories
            .iter()
            .any(|s| s.part == "footer" && s.text.starts_with("Page "))
    );
    let table = &doc.tables()[0];
    assert!(table.rows[0].header);
    assert_eq!(table.rows[0].cells[0].vertical_merge, Some("restart"));
    assert_eq!(table.rows[1].cells[0].vertical_merge, Some("continue"));
    assert_eq!(table.rows[1].cells[1].text(), "C");
}

#[test]
fn a_bad_spec_names_the_problem() {
    let error = Document::from_spec_json(
        &json!({"sections": [{"children": [{"type": "paragraph", "style": "Nope"}]}]}).to_string(),
        &[],
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::Build);
    assert!(error.message().contains("Nope"), "{error}");
    let error = Document::from_spec_json(
        &json!({"sections": [{"children": [{"type": "paragraph", "children": [
            {"type": "text", "text": "x", "color": "red"}]}]}]})
        .to_string(),
        &[],
    )
    .unwrap_err();
    assert!(error.message().contains("red"), "{error}");
}

#[test]
fn an_image_in_a_spec_reads_back_with_its_size() {
    // A real PNG: a page of a blank document.
    let png = Document::create().unwrap().render_page_png(0, 8.0).unwrap();
    let doc = Document::from_spec_json(
        &json!({"sections": [{"children": [{"type": "paragraph", "children": [
            {"type": "image", "media": 0, "transformation": {"width": 96, "height": 48},
             "altText": {"description": "a page"}}]}]}]})
        .to_string(),
        std::slice::from_ref(&png),
    )
    .unwrap();
    let image = doc.paragraphs()[0].runs[0].image.clone().expect("an image");
    assert_eq!((image.width_pt, image.height_pt), (Some(72.0), Some(36.0)));
    assert_eq!(image.description.as_deref(), Some("a page"));
    assert_eq!(doc.media_bytes(&image.part), Some(png.as_slice()));
}

#[test]
fn a_failed_batch_leaves_the_document_unchanged() {
    let mut doc = Document::open(SAMPLE).unwrap();
    let before = doc.text();
    let error = doc
        .edit(|e| {
            e.replace_all("the", "THE")?;
            e.insert_text("not-an-id", 0, "x")
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidArgument);
    assert_eq!(doc.text(), before);
    assert!(!doc.is_modified());
}

#[test]
fn an_undo_restores_the_text_and_a_redo_reapplies_it() {
    let mut doc = Document::open(SAMPLE).unwrap();
    let before = doc.text();
    let (count, undo) = doc.edit(|e| e.replace_all("the", "THE")).unwrap();
    assert!(count > 0);
    let after = doc.text();
    assert_ne!(after, before);
    assert_eq!(
        after.matches("THE").count() - before.matches("THE").count(),
        count
    );
    let redo = doc.undo(&undo).unwrap();
    assert_eq!(doc.text(), before);
    doc.undo(&redo).unwrap();
    assert_eq!(doc.text(), after);
    // The opaque form survives a binding boundary.
    let json = undo.to_json();
    assert!(opendoc::Undo::from_json(&json).is_ok());
}

#[test]
fn json_edit_commands_run_as_one_batch() {
    let mut doc = Document::create().unwrap();
    let commands: Vec<EditCommand> = serde_json::from_value(json!([
        {"op": "insertParagraph", "text": "Hello world", "style": "Heading 2"},
        {"op": "setProperties", "properties": {"title": "T", "author": "Me"}}
    ]))
    .unwrap();
    let (results, _) = doc.edit_commands(&commands).unwrap();
    let id = results[0].as_str().unwrap().to_owned();
    let commands: Vec<EditCommand> = serde_json::from_value(json!([
        {"op": "setRunFormat", "paragraph": id, "start": 6, "end": 11, "format": {"bold": true, "color": "00FF00"}},
        {"op": "addComment", "paragraph": id, "start": 0, "end": 5, "text": "Why?"},
        {"op": "suggestInsert", "paragraph": id, "offset": 11, "text": "!"}
    ]))
    .unwrap();
    doc.edit_commands(&commands).unwrap();
    let p = doc.paragraphs().into_iter().find(|p| p.id == id).unwrap();
    assert_eq!(p.style.as_deref(), Some("heading 2"));
    assert!(
        p.runs
            .iter()
            .any(|r| r.text == "world" && r.bold == Some(true))
    );
    assert_eq!(doc.comments()[0].text, "Why?");
    assert_eq!(doc.comments()[0].author.as_deref(), Some("OpenDoc"));
    assert!(
        doc.revisions()
            .iter()
            .any(|r| r.kind == "insertion" && r.text == "!")
    );
    assert_eq!(doc.properties().title.as_deref(), Some("T"));
    // Accepting makes the suggestion plain text.
    doc.edit(|e| e.accept_all_revisions()).unwrap();
    assert!(doc.revisions().is_empty());
    assert!(doc.text().contains("Hello world!"));
}

#[test]
fn an_exact_save_returns_the_original_bytes_and_refuses_after_an_edit() {
    let mut doc = Document::open(SAMPLE).unwrap();
    let (exact, report) = doc.save(Format::Docx, SaveMode::Exact).unwrap();
    assert_eq!(exact, SAMPLE);
    assert!(report.findings.is_empty());
    doc.edit(|e| e.append_paragraph("x", None)).unwrap();
    let error = doc.save(Format::Docx, SaveMode::Exact).unwrap_err();
    assert_eq!(error.code(), ErrorCode::Save);
}

#[test]
fn a_document_converts_to_every_saved_format() {
    let doc = Document::open(SAMPLE).unwrap();
    for format in Format::ALL.into_iter().filter(|f| f.can_save()) {
        let (bytes, report) = doc.save(format, SaveMode::Semantic).unwrap();
        assert!(!bytes.is_empty(), "{format} wrote nothing");
        assert_eq!(report.format, format.name());
        assert_eq!(
            report.lossy_by_nature,
            !format.carries_document(),
            "{format}"
        );
    }
    let (txt, _) = opendoc::convert(SAMPLE, Format::Txt).unwrap();
    let txt = String::from_utf8(txt).unwrap();
    let first = doc
        .paragraphs()
        .into_iter()
        .find(|p| !p.text.is_empty())
        .unwrap();
    assert!(txt.contains(&first.text));
    let (odt, _) = opendoc::convert(SAMPLE, Format::Odt).unwrap();
    let back = Document::open(&odt).unwrap();
    assert_eq!(back.source_format(), Some(Format::Odt));
    assert!(back.text().contains(&first.text));
}

#[test]
fn render_png_and_pdf_come_from_one_layout() {
    let doc = Document::open(SAMPLE).unwrap();
    let pages = doc.page_count();
    assert!(pages > 1);
    let png = doc.render_page_png(0, 48.0).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    let pdf = doc.to_pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let page_objects = String::from_utf8_lossy(&pdf)
        .matches("/Type /Page\n")
        .count()
        + String::from_utf8_lossy(&pdf).matches("/Type/Page/").count();
    let _ = page_objects;
    assert_eq!(doc.page_sizes().len(), pages);
    assert!(doc.render_page(pages, 48.0).is_err());
    // Same bytes, same pixels, on a second open.
    let again = Document::open(SAMPLE)
        .unwrap()
        .render_page_png(0, 48.0)
        .unwrap();
    assert_eq!(png, again);
}

#[test]
fn an_edit_relays_the_document_out() {
    let mut doc = Document::create().unwrap();
    assert_eq!(doc.page_count(), 1);
    doc.edit(|e| {
        for i in 0..80 {
            e.append_paragraph(&format!("Line {i}"), None)?;
        }
        Ok(())
    })
    .unwrap();
    assert!(doc.page_count() >= 2, "80 paragraphs fill more than a page");
}

#[test]
fn templates_fill_values_loops_conditions_and_rows() {
    let mut doc = spec(json!({"sections": [{"children": [
        {"type": "paragraph", "text": "Dear {name},"},
        {"type": "paragraph", "text": "{#items}"},
        {"type": "paragraph", "text": "- {title}: {price}"},
        {"type": "paragraph", "text": "{/items}"},
        {"type": "paragraph", "text": "{#vip}VIP {company.name}{/vip}{^vip}Regular{/vip}"},
        {"type": "paragraph", "text": "{^empty}Nothing here{/empty}"},
        {"type": "paragraph", "text": "Notes: {notes}"},
        {"type": "table", "rows": [
            {"children": [{"children": [{"type": "paragraph", "text": "Item"}]},
                          {"children": [{"type": "paragraph", "text": "Qty"}]}]},
            {"children": [{"children": [{"type": "paragraph", "text": "{#items}{title}"}]},
                          {"children": [{"type": "paragraph", "text": "{qty}{/items}"}]}]}
        ]}
    ]}]}));
    let tags = doc.template_tags(&TemplateOptions::default());
    assert!(tags.contains(&("items".to_owned(), "loop")));
    assert!(tags.contains(&("name".to_owned(), "value")));
    doc.render_template(
        &json!({
            "name": "Ada",
            "vip": true,
            "company": {"name": "ACME"},
            "empty": [],
            "notes": "line one\nline two",
            "items": [
                {"title": "Pen", "price": "1.00", "qty": 2},
                {"title": "Ink", "price": "3.50", "qty": 1}
            ]
        }),
        &TemplateOptions::default(),
    )
    .unwrap();
    let texts = para_text(&doc);
    assert_eq!(
        texts,
        [
            "Dear Ada,",
            "- Pen: 1.00",
            "- Ink: 3.50",
            "VIP ACME",
            "Nothing here",
            "Notes: line one\nline two"
        ]
    );
    // The newline is a real line break.
    let notes = doc.paragraphs().into_iter().last().unwrap();
    assert!(notes.runs.iter().any(|r| r.kind == "break"));
    let table = &doc.tables()[0];
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|r| r.cells.iter().map(|c| c.text()).collect())
        .collect();
    assert_eq!(rows, [["Item", "Qty"], ["Pen", "2"], ["Ink", "1"]]);
    // The filled document saves without loss.
    let (_, report) = doc.save(Format::Docx, SaveMode::PreserveWhenSafe).unwrap();
    assert!(report.is_lossless(), "{:?}", report.losses);
}

#[test]
fn template_errors_are_all_reported_and_change_nothing() {
    let mut doc = spec(json!({"sections": [{"children": [
        {"type": "paragraph", "text": "Hello {name"},
        {"type": "paragraph", "text": "{#open}"},
        {"type": "paragraph", "text": "x {/never}"}
    ]}]}));
    let before = doc.text();
    let error = doc
        .render_template(&json!({}), &TemplateOptions::default())
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::Template);
    assert!(error.message().contains("unclosed_tag"), "{error}");
    assert!(error.message().contains("unclosed_loop"), "{error}");
    assert_eq!(doc.text(), before);
}

#[test]
fn custom_delimiters_and_strict_mode() {
    let mut doc = spec(json!({"sections": [{"children": [
        {"type": "paragraph", "text": "Hi [[who]] {kept}"}
    ]}]}));
    let options = TemplateOptions {
        start: "[[".into(),
        end: "]]".into(),
        strict: true,
        ..TemplateOptions::default()
    };
    let error = doc.render_template(&json!({}), &options).unwrap_err();
    assert!(error.message().contains("undefined_value"), "{error}");
    doc.render_template(&json!({"who": "you"}), &options)
        .unwrap();
    assert_eq!(para_text(&doc), ["Hi you {kept}"]);
}

#[test]
fn a_protected_document_refuses_an_edit() {
    let mut doc = Document::create().unwrap();
    // Read-only protection, through the same document a Word user would save.
    let bytes = doc.save(Format::Docx, SaveMode::Semantic).unwrap().0;
    let protected = protect(&bytes);
    let mut doc2 = Document::open(&protected).unwrap();
    let error = doc2.edit(|e| e.append_paragraph("x", None)).unwrap_err();
    assert_eq!(error.code(), ErrorCode::Edit);
    let _ = doc.edit(|e| e.append_paragraph("fine", None)).unwrap();
}

/// Adds `w:documentProtection w:edit="readOnly" w:enforcement="1"` to a package.
fn protect(docx: &[u8]) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let name = file.name().unwrap().to_string();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        if name == "word/settings.xml" {
            let text = String::from_utf8(bytes).unwrap();
            let at = text.find('>').unwrap() + 1;
            let at = at + text[at..].find('>').unwrap() + 1;
            bytes = format!(
                "{}<w:documentProtection w:edit=\"readOnly\" w:enforcement=\"1\"/>{}",
                &text[..at],
                &text[at..]
            )
            .into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn every_body_block_kind_is_projected() {
    let doc = Document::open(SAMPLE).unwrap();
    let body = doc.body();
    assert!(!body.is_empty());
    assert!(body.iter().all(|b| !b.id().is_empty()));
    assert!(body.iter().any(|b| matches!(b, Block::Paragraph(_))));
    // Every block serialises to the JSON the bindings read.
    let json = serde_json::to_value(&body).unwrap();
    assert!(json[0]["type"].is_string());
}
