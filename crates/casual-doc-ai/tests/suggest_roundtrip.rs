// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §3, guard G5: every command the registry declares trackable
//! round-trips as a suggestion.
//!
//! One generic test, not one per command. For each `COMMANDS` entry with
//! `Trackable::Yes` it takes the command's own registry example, points its
//! placeholder ids at a fixture built from `EXAMPLE_PARAGRAPH`, applies it in
//! suggest mode, saves DOCX, reopens, and asserts that the change reads back
//! as revisions by the session's author and that rejecting every revision
//! restores what the document said and how it was formatted. A command that
//! is newly registered as trackable is covered by this test the moment it is
//! registered: there is nothing to add here.

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat};
use casual_doc_ai::{Clock, ExportFormat, Profile, Session, SessionConfig};
use casual_doc_command::registry::{EXAMPLE_BOOKMARK, EXAMPLE_IDS, EXAMPLE_PARAGRAPH};
use casual_doc_command::{COMMANDS, Trackable};
use casual_doc_model::v1::{BlockNode, InlineNode};
use serde_json::{Value, json};

const AUTHOR: &str = "Claude (AI)";

fn open(bytes: &[u8], name: &str, profile: Profile) -> Session {
    let config = SessionConfig {
        author: AUTHOR.to_owned(),
        profile,
        clock: Clock::Fixed("2026-10-28T00:00:00Z".to_owned()),
        ..SessionConfig::default()
    };
    Session::open(bytes, name, config).expect("the fixture opens")
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

fn save(session: &Session) -> Vec<u8> {
    session
        .export(ExportFormat::Docx, &[])
        .expect("the DOCX save loses nothing")
        .bytes
}

/// The paragraph the fixture's picture sits in.
const PICTURE_PARAGRAPH: &str = "Signed:";

/// A 1x1 PNG, as the `insert_image` example carries it.
const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";

/// The ids the examples' placeholders stand for, in `EXAMPLE_IDS` order: the
/// example paragraph, the numbered item after it, the table, and the picture.
fn ids(session: &Session) -> [String; 4] {
    let blocks = read::read_blocks(session, None, None, Some(20_000), ReadFormat::Json)
        .expect("the body reads");
    let blocks = blocks["blocks"].as_array().expect("blocks").clone();
    let id_of = |pick: &dyn Fn(&Value) -> bool| {
        blocks
            .iter()
            .find(|block| pick(block))
            .and_then(|block| block["id"].as_str())
            .expect("the fixture block")
            .to_owned()
    };
    let picture = session
        .document()
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.inlines.iter().find_map(|inline| match inline {
                    InlineNode::Drawing(drawing) => Some(drawing.id.to_string()),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap_or_default();
    [
        id_of(&|b| b["text"] == EXAMPLE_PARAGRAPH),
        id_of(&|b| b["text"] == "First item"),
        id_of(&|b| b["kind"] == "table"),
        picture,
    ]
}

/// `example` with its placeholder ids pointed at `session`'s blocks.
fn example_for(session: &Session, example: &str) -> Value {
    let mut example = example.to_owned();
    for (placeholder, id) in EXAMPLE_IDS.iter().zip(ids(session)) {
        example = example.replace(&format!("\"{placeholder}\""), &format!("\"{id}\""));
    }
    serde_json::from_str(&example).expect("the example is JSON")
}

/// A heading the fixture's table of contents lists, as authored; the fixture
/// renames it afterwards, so the contents is stale and an `entire` update has
/// something to change. A second heading follows it: a one-entry contents field
/// begins and ends in one paragraph, and Word's markup for that reopens as an
/// inline field rather than a paragraph-spanning one.
const HEADING: &str = "Terms";

/// A minimal Word file: the example paragraph and "First item", with the
/// built-in Normal and heading styles the examples name; then a heading and a
/// caption whose `SEQ` field shows a stale number, as a document another
/// producer last saved would (`update_caption_numbers` has one to correct).
fn word_file() -> Vec<u8> {
    use std::io::Write;
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
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
                 <w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>\
                 <w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"26\"/></w:rPr></w:style>\
                 </w:styles>"
            ),
        ),
        (
            "word/document.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <w:document xmlns:w=\"{W}\"><w:body>\
                 <w:p><w:r><w:t xml:space=\"preserve\">{EXAMPLE_PARAGRAPH}</w:t></w:r></w:p>\
                 <w:p><w:r><w:t>First item</w:t></w:r></w:p>\
                 <w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>{HEADING}</w:t></w:r></w:p>\
                 <w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Payment</w:t></w:r></w:p>\
                 <w:p><w:r><w:t xml:space=\"preserve\">Figure </w:t></w:r><w:fldSimple w:instr=\" SEQ Figure \\* ARABIC \"><w:r><w:t>7</w:t></w:r></w:fldSimple><w:r><w:t xml:space=\"preserve\">: Site plan</w:t></w:r></w:p>\
                 <w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"/></w:sectPr>\
                 </w:body></w:document>"
            ),
        ),
    ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .expect("a part");
        zip.write_all(body.as_bytes()).expect("written");
    }
    zip.finish().expect("a zip").into_inner()
}

/// The fixture: a table of contents, the example paragraph, a numbered item
/// (with a hyperlink over `First` and the bookmark `EXAMPLE_BOOKMARK` over
/// `item`), a filled 2x2 table, a paragraph holding a picture, a heading the
/// contents no longer matches, and a caption showing a stale number, saved as
/// DOCX so every run starts from a Word file. Built through the registry's own
/// commands.
fn fixture() -> Vec<u8> {
    let mut session = open(&word_file(), "examples.docx", Profile::Edit);
    let blocks = read::read_blocks(&session, None, None, Some(20_000), ReadFormat::Json)
        .expect("the body reads");
    let item = blocks["blocks"][1]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    apply(
        &mut session,
        "direct",
        json!([
            { "op": "insert_paragraphs", "after": { "node": item },
              "paragraphs": [{ "text": PICTURE_PARAGRAPH }] },
            { "op": "set_list", "target": { "node": item }, "kind": "numbered" },
            { "op": "insert_table", "after": { "node": item }, "rows": 2, "columns": 2 },
            { "op": "set_hyperlink", "target": { "node": item, "exact": "First" },
              "url": "https://example.com/first" },
            { "op": "create_bookmark", "target": { "node": item, "exact": "item" },
              "name": EXAMPLE_BOOKMARK },
        ]),
    )
    .expect("the fixture builds");
    let blocks = read::read_blocks(&session, None, None, Some(20_000), ReadFormat::Json)
        .expect("the body reads");
    let signed = blocks["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .find(|block| block["text"] == PICTURE_PARAGRAPH)
        .and_then(|block| block["id"].as_str())
        .expect("the picture paragraph")
        .to_owned();
    apply(
        &mut session,
        "direct",
        json!([{ "op": "insert_image", "at": { "node": signed, "side": "after" },
                 "data": PNG_BASE64, "content_type": "image/png", "width_pt": 36 }]),
    )
    .expect("the picture inserts");
    let [_, _, table, _] = ids(&session);
    let cells: Vec<Value> = (0..2)
        .flat_map(|row| (0..2).map(move |column| (row, column)))
        .map(|(row, column)| {
            json!({ "op": "set_cell_text", "table": table, "row": row, "column": column,
                    "text": format!("r{row}c{column}") })
        })
        .collect();
    apply(&mut session, "direct", Value::Array(cells)).expect("the cells fill");
    let [example, ..] = ids(&session);
    let blocks = read::read_blocks(&session, None, None, Some(20_000), ReadFormat::Json)
        .expect("the body reads");
    let heading = blocks["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .find(|block| block["text"] == HEADING)
        .and_then(|block| block["id"].as_str())
        .expect("the heading")
        .to_owned();
    apply(
        &mut session,
        "direct",
        json!([
            { "op": "insert_table_of_contents", "before": { "node": example } },
            { "op": "replace_text", "target": { "node": heading, "exact": HEADING },
              "text": "Terms and conditions" },
        ]),
    )
    .expect("the contents builds, then goes stale");
    save(&session)
}

/// What the document says and how its paragraphs are formatted, without
/// ids (a reopen re-mints them) and without pending-change records.
fn state(session: &Session) -> Vec<String> {
    let mut out = Vec::new();
    for block in session.document().body() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut properties = paragraph.properties.get().clone();
                properties.prop_change = None;
                let numbering = properties.numbering.take().map(|n| n.level);
                let style = properties.style_ref.take().and_then(|style| {
                    session
                        .document()
                        .definitions()
                        .styles
                        .get(&style)
                        .and_then(|definition| definition.name.clone())
                });
                let runs = casual_doc_ai::text::runs(paragraph, 1_000);
                out.push(format!(
                    "p {:?} style={style:?} list={numbering:?} props={} runs={}",
                    casual_doc_ai::text::paragraph_text(paragraph),
                    serde_json::to_string(&properties).expect("serialises"),
                    serde_json::to_string(&runs).expect("serialises"),
                ));
            }
            BlockNode::Table(table) => {
                let read = read::read_table(session, &table.id.to_string(), None, None)
                    .expect("the table reads");
                let cells: Vec<Value> = read["rows"]
                    .as_array()
                    .expect("rows")
                    .iter()
                    .map(|row| {
                        row["cells"]
                            .as_array()
                            .expect("cells")
                            .iter()
                            .map(|cell| cell["text"].clone())
                            .collect()
                    })
                    .collect();
                out.push(format!("table {}", Value::Array(cells)));
            }
            BlockNode::Sdt(_) | BlockNode::AltChunk(_) => out.push("other".to_owned()),
        }
    }
    out
}

#[test]
fn every_trackable_command_round_trips_as_a_suggestion_and_rejecting_it_restores_the_document() {
    let base = fixture();
    let original = state(&open(&base, "base.docx", Profile::Review));
    let trackable: Vec<_> = COMMANDS
        .iter()
        .filter(|command| command.trackable == Trackable::Yes)
        .collect();
    assert!(
        trackable.len() >= 10,
        "the guard runs over the registry, so it must see the trackable commands"
    );
    let mut failures = Vec::new();
    for command in trackable {
        let mut session = open(&base, "base.docx", Profile::Suggest);
        let edit = example_for(&session, command.example);
        if let Err(error) = apply(&mut session, "suggest", json!([edit])) {
            failures.push(format!(
                "{}: refused in suggest mode: {error:?}",
                command.id
            ));
            continue;
        }
        let saved = save(&session);
        let mut reopened = open(&saved, "suggested.docx", Profile::Review);
        let changes =
            read::list_tracked_changes(&reopened, None, Some(500)).expect("the changes read");
        let changes = changes["trackedChanges"]
            .as_array()
            .expect("a list")
            .clone();
        if changes.is_empty() {
            failures.push(format!(
                "{}: no revision survived the DOCX save",
                command.id
            ));
            continue;
        }
        if let Some(stranger) = changes.iter().find(|change| change["author"] != AUTHOR) {
            failures.push(format!(
                "{}: a revision is not the session's: {stranger}",
                command.id
            ));
        }
        if state(&reopened) == original {
            failures.push(format!(
                "{}: the suggestion changed nothing visible",
                command.id
            ));
        }
        if let Err(error) = apply(
            &mut reopened,
            "suggest",
            json!([{ "op": "review", "decision": "reject", "all": true }]),
        ) {
            failures.push(format!("{}: reject-all refused: {error:?}", command.id));
            continue;
        }
        let after = state(&reopened);
        if after != original {
            failures.push(format!(
                "{}: reject-all did not restore the document:\n  was {original:?}\n  now {after:?}",
                command.id
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Examples that cannot change the fixture, each with what they must still
/// do: `Some(text)` is the refusal they produce (so the edit parsed and
/// resolved, and was refused for that reason, not for a malformed field);
/// `None` is an edit that applies and has nothing to change.
const DIRECT_EXCEPTIONS: &[(&str, Option<&str>)] = &[
    // The example text carries no direct formatting in the fixture.
    ("clear_formatting", None),
    // The fixture's table has no merged cell.
    ("table_split_cell", Some("not merged")),
    // The fixture has one numbered list, so there is none earlier to join.
    ("continue_numbering", Some("no earlier numbered list")),
    // The fixture's picture is inline; agents cannot float one yet
    // (`layout.arrange.inLine` is not exposed).
    ("set_wrap", Some("Only a floating object has text wrapping")),
    // A fill belongs to a shape; agents cannot insert one yet.
    ("set_shape_fill", Some("engine")),
];

/// `docs/181` §12 (lane families-b): every command that is not trackable runs
/// its own registry example for real, in direct mode, against the fixture, and
/// the result saves to DOCX without loss and reopens. G5 covers the trackable
/// ones; this covers the rest, so a registry example is a working edit, and a
/// lowering that writes something the writer cannot save (a picture whose bytes
/// never reached the package) fails here.
#[test]
fn every_untracked_command_applies_directly_saves_and_reopens() {
    let base = fixture();
    let untracked: Vec<_> = COMMANDS
        .iter()
        .filter(|command| matches!(command.trackable, Trackable::No(_)))
        .collect();
    assert!(
        untracked.len() >= 30,
        "the guard runs over the registry, so it must see the untracked commands"
    );
    let mut failures = Vec::new();
    for command in untracked {
        let mut session = open(&base, "base.docx", Profile::Edit);
        let before = session.digest();
        let edit = example_for(&session, command.example);
        let result = apply(&mut session, "direct", json!([edit]));
        if let Some((_, expected)) = DIRECT_EXCEPTIONS.iter().find(|(id, _)| *id == command.id) {
            match (result, expected) {
                (Ok(_), None) if session.digest() == before => {}
                (Err(error), Some(reason)) => {
                    let text = format!("{error:?}");
                    if text.contains("malformed") || !text.contains(reason) {
                        failures.push(format!(
                            "{}: refused for another reason: {text}",
                            command.id
                        ));
                    }
                }
                (outcome, _) => failures.push(format!(
                    "{}: listed as unable to change the fixture, but: {outcome:?}; drop the \
                     exception",
                    command.id
                )),
            }
            continue;
        }
        if let Err(error) = result {
            failures.push(format!("{}: refused in direct mode: {error:?}", command.id));
            continue;
        }
        if session.digest() == before {
            failures.push(format!("{}: applied but changed nothing", command.id));
            continue;
        }
        let saved = match session.export(ExportFormat::Docx, &[]) {
            Ok(exported) => exported.bytes,
            Err(error) => {
                failures.push(format!("{}: the DOCX save refused: {error:?}", command.id));
                continue;
            }
        };
        if let Err(error) = Session::open(&saved, "direct.docx", SessionConfig::default()) {
            failures.push(format!(
                "{}: the saved DOCX does not reopen: {error:?}",
                command.id
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The picture an agent inserts reaches the package: its bytes are the part the
/// drawing names, after a save and a reopen.
#[test]
fn an_agent_picture_is_saved_with_its_bytes() {
    let base = fixture();
    let session = open(&base, "base.docx", Profile::Review);
    let picture = session
        .document()
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.inlines.iter().find_map(|inline| match inline {
                    InlineNode::Drawing(drawing) => Some(drawing.media),
                    _ => None,
                })
            }
            _ => None,
        })
        .expect("the fixture's picture survived the save");
    let part = session
        .document()
        .definitions()
        .media
        .get(&picture)
        .map(|reference| reference.part_name.clone())
        .expect("the picture names its media part");
    let png = casual_doc_command::objects::decode_base64(PNG_BASE64, 1024)
        .expect("the example is base64");
    assert_eq!(
        session.resource(&part),
        Some(png.as_slice()),
        "the reopened package holds the picture's bytes at {part}"
    );
}
