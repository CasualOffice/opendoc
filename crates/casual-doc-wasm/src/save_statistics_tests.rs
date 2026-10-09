// SPDX-License-Identifier: Apache-2.0

//! An edited save states the document's CURRENT counts in `docProps/app.xml`
//! (`docs/109` FID-AT-04): the engine lane's save leaves out every statistic an
//! edit made stale, and the editor, which knows the counts (the status bar shows
//! them), writes them instead — without the save changing the document the
//! session goes on editing.

use casual_doc_edit::Operation;
use casual_doc_edit::Pos;

use crate::{DocxPackage, WasmDocument, open_document, viewer_limits};

const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");
const DOCX: &str = casual_doc_io::formats::DOCX;

fn part_text(bytes: &[u8], part: &str) -> String {
    let mut package = DocxPackage::open(bytes, viewer_limits()).expect("open package");
    String::from_utf8(package.read_part(part).expect("read part")).expect("utf-8")
}

/// `<Tag>n</Tag>` in `app.xml`, or `None` when the save left it out.
fn statistic(app: &str, tag: &str) -> Option<i64> {
    let open = format!("<{tag}>");
    let start = app.find(&open)? + open.len();
    let end = start + app[start..].find('<')?;
    Some(app[start..end].parse().expect("a number"))
}

fn save(d: &mut WasmDocument) -> (String, String) {
    let artifact = d
        .export_as_inner(DOCX, "preserve_when_safe")
        .expect("the save exports");
    (
        part_text(&artifact.bytes, "docProps/app.xml"),
        artifact.report_json,
    )
}

#[test]
fn an_edited_save_states_the_documents_current_counts_and_changes_nothing_else() {
    let mut d = open_document(SAMPLE_DOCX).expect("open sample.docx");
    // Word's file says 0 words and 1 page: a template's counts, which describe
    // nothing about the text the reader sees.
    let source = d
        .document
        .properties()
        .expect("sample.docx has app.xml")
        .app
        .clone();
    assert_eq!(source.words, Some(0));
    assert_eq!(source.pages, Some(1));

    // Unedited, the save keeps the file's own counts — they are the file's, and
    // nothing has made them stale.
    let (app, _) = save(&mut d);
    assert_eq!(
        statistic(&app, "Words"),
        Some(0),
        "an unedited save keeps Word's counts"
    );

    let first = d.ordered_paragraphs()[0].0;
    d.apply(Operation::InsertText {
        at: Pos::new(first, 0),
        text: "alpha beta gamma ".to_owned(),
    })
    .expect("type three words");
    let stats = d.document_stats();
    let (app, report) = save(&mut d);

    assert_eq!(
        statistic(&app, "Words"),
        Some(i64::from(stats.words())),
        "the save states the word count the status bar shows: {app}"
    );
    assert_eq!(
        statistic(&app, "Characters"),
        Some(i64::from(stats.characters()))
    );
    assert_eq!(
        statistic(&app, "CharactersWithSpaces"),
        Some(i64::from(stats.characters_with_spaces()))
    );
    assert_eq!(
        statistic(&app, "Paragraphs"),
        Some(i64::from(stats.paragraphs()))
    );
    assert_eq!(
        statistic(&app, "Pages"),
        Some(i64::from(d.page_count())),
        "the page count is the session's own, and it is exact for a whole body"
    );
    let lines = statistic(&app, "Lines").expect("a whole body knows its line count");
    assert!(
        lines >= i64::from(stats.paragraphs()) / 2 && lines > 0,
        "a plausible line count: {lines}"
    );
    assert!(
        !report.contains("docx.export.stale.statistics"),
        "nothing is left out as stale, because nothing written is: {report}"
    );

    // Saving changed nothing in the document the session goes on editing.
    assert_eq!(
        d.document.properties().expect("still there").app,
        source,
        "the counts were written for the save only"
    );
}
