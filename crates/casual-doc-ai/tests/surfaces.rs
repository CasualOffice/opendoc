// SPDX-License-Identifier: Apache-2.0

//! `docs/181` R2: every surface is discoverable and readable, what a paragraph
//! holds besides its text is projected (or named and counted as omitted), and
//! the exit gate: an agent reads and suggests into a header and a footnote, the
//! DOCX save carries each revision in that part, and it reopens as revisions.

use std::io::{Read, Write};

use casual_doc_ai::anchor::{self, Anchor};
use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat, Scope, SearchMode};
use casual_doc_ai::{Clock, ExportFormat, Profile, Session, SessionConfig, Surface};
use serde_json::{Value, json};

const AUTHOR: &str = "Claude (AI)";

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NS: &str = concat!(
    r#" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#,
    r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#,
    r#" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing""#,
    r#" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#,
    r#" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#,
    r#" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#,
    r#" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#,
);
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

/// A 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Body paragraphs: a note-citing paragraph, a bookmarked hyperlink and a
/// PAGE field, a commented range, a block content control, a picture with alt
/// text, and two equations (one with a linear text form, one without).
const BODY: &str = concat!(
    r#"<w:p><w:r><w:t xml:space="preserve">The body cites a note</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:t xml:space="preserve"> and an endnote</w:t></w:r><w:r><w:endnoteReference w:id="1"/></w:r></w:p>"#,
    r#"<w:p><w:bookmarkStart w:id="0" w:name="terms"/><w:r><w:t xml:space="preserve">See </w:t></w:r><w:hyperlink r:id="rIdL"><w:r><w:t>the terms</w:t></w:r></w:hyperlink><w:bookmarkEnd w:id="0"/><w:r><w:t xml:space="preserve"> on page </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p>"#,
    r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>Commented text</w:t></w:r><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r></w:p>"#,
    r#"<w:sdt><w:sdtPr><w:alias w:val="Party name"/><w:tag w:val="party"/><w:text/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Acme Ltd</w:t></w:r></w:p></w:sdtContent></w:sdt>"#,
    r#"<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="914400" cy="914400"/><wp:docPr id="2" name="Picture 2" descr="A red square logo"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="2" name="logo.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
    r#"<w:p><w:r><w:t xml:space="preserve">Area </w:t></w:r><m:oMath><m:r><m:t>x=2</m:t></m:r></m:oMath></w:p>"#,
    r#"<w:p><w:r><w:t xml:space="preserve">Root </w:t></w:r><m:oMath><m:rad><m:radPr><m:degHide m:val="1"/></m:radPr><m:deg/><m:e/></m:rad></m:oMath></w:p>"#,
);

/// An inline DrawingML text box. The importer models it (its paragraphs are a
/// surface) but also reports the drawing as omitted, so a DOCX save of a
/// document holding one is refused by the loss gate unless the loss is
/// accepted; the save test therefore uses a package without it.
const TEXT_BOX: &str = r#"<w:p><w:r><w:t xml:space="preserve">Box: </w:t></w:r><w:r><w:drawing><wp:inline><wp:extent cx="1828800" cy="914400"/><a:graphic><a:graphicData><wps:wsp><wps:spPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Inside the box</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;

/// A package with a body, a default header and footer, one footnote, one
/// endnote and one comment, and (when `text_box`) a text box: one of every
/// surface, and every inline family R2 projects.
fn surfaces_docx(text_box: bool) -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="{CT}.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="{CT}.header+xml"/><Override PartName="/word/footer1.xml" ContentType="{CT}.footer+xml"/><Override PartName="/word/footnotes.xml" ContentType="{CT}.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="{CT}.endnotes+xml"/><Override PartName="/word/comments.xml" ContentType="{CT}.comments+xml"/></Types>"#
    );
    let root_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#
    );
    let doc_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="{REL}/header" Target="header1.xml"/><Relationship Id="rIdF" Type="{REL}/footer" Target="footer1.xml"/><Relationship Id="rIdFn" Type="{REL}/footnotes" Target="footnotes.xml"/><Relationship Id="rIdEn" Type="{REL}/endnotes" Target="endnotes.xml"/><Relationship Id="rIdC" Type="{REL}/comments" Target="comments.xml"/><Relationship Id="rIdImg" Type="{REL}/image" Target="media/logo.png"/><Relationship Id="rIdL" Type="{REL}/hyperlink" Target="https://example.com/terms" TargetMode="External"/></Relationships>"#
    );
    let boxed = if text_box { TEXT_BOX } else { "" };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document{NS}><w:body>{BODY}{boxed}<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:footerReference w:type="default" r:id="rIdF"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="{W}"><w:p><w:r><w:t>Confidential draft header</w:t></w:r></w:p></w:hdr>"#
    );
    let footer = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr xmlns:w="{W}"><w:p><w:r><w:t>Footer line</w:t></w:r></w:p></w:ftr>"#
    );
    let footnotes = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes xmlns:w="{W}"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> The footnote says thirty days.</w:t></w:r></w:p></w:footnote></w:footnotes>"#
    );
    let endnotes = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:endnotes xmlns:w="{W}"><w:endnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote><w:endnote w:id="1"><w:p><w:r><w:endnoteRef/></w:r><w:r><w:t xml:space="preserve"> The endnote text.</w:t></w:r></w:p></w:endnote></w:endnotes>"#
    );
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments xmlns:w="{W}"><w:comment w:id="0" w:author="Reviewer" w:initials="R" w:date="2026-10-01T00:00:00Z"><w:p><w:r><w:t>Check this clause.</w:t></w:r></w:p></w:comment></w:comments>"#
    );
    let parts: [(&str, &str); 9] = [
        ("[Content_Types].xml", &content_types),
        ("_rels/.rels", &root_rels),
        ("word/_rels/document.xml.rels", &doc_rels),
        ("word/document.xml", &document),
        ("word/header1.xml", &header),
        ("word/footer1.xml", &footer),
        ("word/footnotes.xml", &footnotes),
        ("word/endnotes.xml", &endnotes),
        ("word/comments.xml", &comments),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in parts {
        zip.start_file(name, SimpleFileOptions::default())
            .expect("zip entry");
        zip.write_all(body.as_bytes()).expect("zip write");
    }
    zip.start_file("word/media/logo.png", SimpleFileOptions::default())
        .expect("zip entry");
    zip.write_all(PNG).expect("zip write");
    zip.finish().expect("zip finish");
    out.into_inner()
}

fn open(bytes: &[u8], profile: Profile) -> Session {
    let config = SessionConfig {
        author: AUTHOR.to_owned(),
        profile,
        clock: Clock::Fixed("2026-10-28T00:00:00Z".to_owned()),
        ..SessionConfig::default()
    };
    Session::open(bytes, "surfaces.docx", config).expect("the fixture opens")
}

fn part(docx: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx)).expect("a zip");
    let mut part = archive.by_name(name).expect("the part");
    let mut xml = String::new();
    part.read_to_string(&mut xml).expect("utf-8");
    xml
}

/// Every surface the outline lists (all pages of it).
fn surfaces(session: &Session) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let outline = read::outline_paged(session, None, None, Some(2), cursor.as_deref())
            .expect("the outline");
        out.extend(outline["surfaces"].as_array().expect("surfaces").clone());
        match outline["surfacesNextCursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => return out,
        }
    }
}

/// The one surface of `kind` the outline lists.
fn surface_of_kind(session: &Session, kind: &str) -> Surface {
    let found: Vec<Value> = surfaces(session)
        .into_iter()
        .filter(|entry| entry["kind"] == kind)
        .collect();
    assert_eq!(found.len(), 1, "one {kind} surface");
    Surface::parse(found[0]["surface"].as_str().expect("a key")).expect("a surface the tools parse")
}

/// The first paragraph id of `surface`, read through the read tool.
fn first_paragraph(session: &Session, surface: Surface) -> (String, String) {
    let read = read::read_blocks_in(session, Some(surface), None, None, None, ReadFormat::Json)
        .expect("the surface reads");
    let block = read["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .find(|block| block["kind"] == "paragraph")
        .expect("a paragraph")
        .clone();
    (
        block["id"].as_str().expect("an id").to_owned(),
        block["text"].as_str().expect("text").to_owned(),
    )
}

#[test]
fn the_outline_lists_every_surface_with_where_it_shows() {
    let session = open(&surfaces_docx(true), Profile::Read);
    let all = surfaces(&session);
    let kinds: Vec<&str> = all.iter().filter_map(|s| s["kind"].as_str()).collect();
    assert_eq!(
        kinds,
        [
            "body", "header", "footer", "footnote", "endnote", "comment", "text_box"
        ],
        "{all:#?}"
    );
    let header = all.iter().find(|s| s["kind"] == "header").expect("header");
    assert_eq!(header["uses"], json!([{ "section": 0, "page": "default" }]));
    assert_eq!(header["preview"], "Confidential draft header");
    assert!(header["tokens"].as_u64().expect("tokens") > 0);
    let footnote = all.iter().find(|s| s["kind"] == "footnote").expect("note");
    assert_eq!(footnote["referencedFrom"]["surface"], "body");
    let text_box = all.iter().find(|s| s["kind"] == "text_box").expect("box");
    assert_eq!(text_box["host"]["surface"], "body");
    let comment = all
        .iter()
        .find(|s| s["kind"] == "comment")
        .expect("comment");
    assert_eq!(comment["editable"], false);
    assert_eq!(comment["author"], "Reviewer");

    let outline = read::outline(&session, None, None, None).expect("outline");
    let counts = &outline["counts"];
    assert_eq!(counts["headers"], 1);
    assert_eq!(counts["footnotes"], 1);
    assert_eq!(counts["textBoxes"], 1);
    assert_eq!(counts["bookmarks"], 1);
    assert_eq!(outline["bookmarks"][0]["name"], "terms");
    assert_eq!(outline["surfaceCount"], 7);
}

#[test]
fn a_header_paragraph_id_is_discoverable_and_resolves_as_an_anchor() {
    let session = open(&surfaces_docx(true), Profile::Read);
    for (kind, quote) in [
        ("header", "draft"),
        ("footer", "Footer"),
        ("footnote", "thirty"),
        ("endnote", "endnote"),
        ("text_box", "box"),
    ] {
        let surface = surface_of_kind(&session, kind);
        let (node, text) = first_paragraph(&session, surface);
        assert!(text.contains(quote), "{kind}: {text}");
        let anchor: Anchor =
            serde_json::from_value(json!({ "node": node, "exact": quote })).expect("an anchor");
        let resolved = anchor::resolve(session.document(), &anchor)
            .unwrap_or_else(|e| panic!("a {kind} paragraph resolves as an anchor: {e:?}"));
        assert_eq!(resolved.node.to_string(), node);
        // A bare `start` from another surface reads in place.
        let read = read::read_blocks(&session, Some(&node), None, None, ReadFormat::Json)
            .expect("read from the id");
        assert_eq!(read["surface"], surface.to_string());
        assert_eq!(read["blocks"][0]["id"], node.as_str());
    }
}

#[test]
fn a_comment_body_reads_but_refuses_edits_by_anchor_with_the_reason() {
    let session = open(&surfaces_docx(true), Profile::Read);
    let surface = surface_of_kind(&session, "comment");
    let (node, text) = first_paragraph(&session, surface);
    assert_eq!(text, "Check this clause.");
    let anchor: Anchor = serde_json::from_value(json!({ "node": node })).expect("an anchor");
    let refused = anchor::resolve(session.document(), &anchor).expect_err("read-only");
    assert_eq!(refused.code, "unsupported-surface");
    assert_eq!(refused.details["surface"], surface.to_string());
}

#[test]
fn a_paragraph_projects_links_fields_bookmarks_notes_controls_and_objects() {
    let session = open(&surfaces_docx(true), Profile::Read);
    let read = read::read_blocks(&session, None, None, Some(20_000), ReadFormat::Json)
        .expect("the body reads");
    let blocks = read["blocks"].as_array().expect("blocks");
    let with = |key: &str| {
        blocks
            .iter()
            .find(|block| block.get(key).is_some())
            .unwrap_or_else(|| panic!("a block with {key}: {read:#}"))
    };
    let link = &with("links")["links"][0];
    assert_eq!(link["url"], "https://example.com/terms");
    assert_eq!(link["text"], "the terms");
    let field = &with("fields")["fields"][0];
    assert_eq!(field["kind"], "page");
    assert_eq!(field["instruction"], "PAGE");
    assert_eq!(field["result"], "1");
    assert_eq!(with("bookmarks")["bookmarks"][0]["name"], "terms");
    let note = &with("notes")["notes"][0];
    assert_eq!(note["kind"], "footnote");
    assert!(
        note["surface"]
            .as_str()
            .expect("a key")
            .starts_with("footnote:")
    );
    assert!(with("comments")["comments"][0].is_string());

    // The block content control keeps its type, tag and alias, and its
    // paragraph names it.
    let control = blocks
        .iter()
        .find(|block| block["kind"] == "content_control")
        .expect("the control");
    assert_eq!(control["tag"], "party");
    assert_eq!(control["alias"], "Party name");
    assert_eq!(control["type"], "plain_text");
    let inside = blocks
        .iter()
        .find(|block| block["text"] == "Acme Ltd")
        .expect("its paragraph");
    assert_eq!(inside["contentControl"], control["id"]);

    let objects: Vec<&Value> = blocks
        .iter()
        .filter_map(|block| block.get("objects"))
        .flat_map(|objects| objects.as_array().expect("objects"))
        .collect();
    let of = |kind: &str| {
        objects
            .iter()
            .find(|object| object["kind"] == kind && object.get("opaque").is_none())
            .unwrap_or_else(|| panic!("a {kind}: {objects:#?}"))
    };
    assert_eq!(of("picture")["alt"], "A red square logo");
    assert_eq!(of("picture")["extentEmu"], json!([914_400, 914_400]));
    assert!(
        of("text_box")["surface"]
            .as_str()
            .expect("a key")
            .starts_with("text_box:")
    );
    assert_eq!(of("equation")["text"], "x=2");

    // The equation with no linear form is named, given a reason, and counted.
    assert_eq!(read["omitted"]["count"], 1, "{:#}", read["omitted"]);
    assert_eq!(read["omitted"]["items"][0]["kind"], "equation");
    assert!(read["omitted"]["items"][0]["reason"].is_string());

    let markdown = read::read_blocks(&session, None, None, Some(20_000), ReadFormat::Markdown)
        .expect("markdown");
    let markdown = markdown["markdown"].as_str().expect("markdown");
    assert!(
        markdown.contains("link \"the terms\" -> https://example.com/terms"),
        "{markdown}"
    );
    assert!(
        markdown.contains("content control plain_text"),
        "{markdown}"
    );
}

#[test]
fn search_and_chunks_cover_one_surface_or_all_of_them() {
    let session = open(&surfaces_docx(true), Profile::Read);
    let body = read::search(&session, "draft", SearchMode::Exact, None, None).expect("search");
    assert_eq!(body["total"], 0, "the default stays the body");

    let header = surface_of_kind(&session, "header");
    let one = read::search_in(
        &session,
        "draft",
        SearchMode::Exact,
        Scope::One(header),
        None,
        None,
    )
    .expect("search the header");
    assert_eq!(one["total"], 1);
    assert_eq!(one["hits"][0]["surface"], header.to_string());

    let all = read::search_in(
        &session,
        "e",
        SearchMode::Exact,
        Scope::All,
        None,
        Some(200),
    )
    .expect("search everything");
    let kinds: std::collections::BTreeSet<String> = all["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .filter_map(|hit| hit["surface"].as_str())
        .map(|key| key.split(':').next().unwrap_or(key).to_owned())
        .collect();
    for kind in [
        "body", "header", "footer", "footnote", "endnote", "comment", "text_box",
    ] {
        assert!(kinds.contains(kind), "search all reaches {kind}: {kinds:?}");
    }

    let chunks = read::chunks_in(&session, None, Scope::All, None, Some(200)).expect("chunks");
    let chunk_surfaces: std::collections::BTreeSet<String> = chunks["chunks"]
        .as_array()
        .expect("chunks")
        .iter()
        .filter_map(|chunk| chunk["surface"].as_str())
        .map(|key| key.split(':').next().unwrap_or(key).to_owned())
        .collect();
    for kind in [
        "body", "header", "footer", "footnote", "endnote", "text_box", "comments",
    ] {
        assert!(
            chunk_surfaces.contains(kind),
            "chunks reach {kind}: {chunk_surfaces:?}"
        );
    }
}

/// The R2 exit gate (`docs/181` §7): an agent finds a header paragraph and a
/// footnote paragraph through the read tools alone, suggests a change into
/// each through the ordinary edit plan, saves DOCX, and each revision is in
/// its own part, by the session's author, and reopens as tracked changes.
#[test]
fn an_agent_suggests_into_a_header_and_a_footnote_and_the_revisions_are_in_those_parts() {
    let mut session = open(&surfaces_docx(false), Profile::Suggest);
    let header = surface_of_kind(&session, "header");
    let footnote = surface_of_kind(&session, "footnote");
    let (header_node, _) = first_paragraph(&session, header);
    let (note_node, _) = first_paragraph(&session, footnote);
    let rev = session.revision();
    let request: EditPlan = serde_json::from_value(json!({
        "base_revision": rev,
        "edits": [
            { "op": "replace_text", "target": { "node": header_node, "exact": "draft" }, "text": "final" },
            { "op": "replace_text", "target": { "node": note_node, "exact": "thirty" }, "text": "sixty" },
        ],
    }))
    .expect("a plan");
    let applied = plan::apply(&mut session, &request).expect("both suggestions apply");
    assert_eq!(applied["mode"], "suggest");

    let docx = session
        .export(ExportFormat::Docx, &[])
        .expect("the save loses nothing");
    let header_xml = part(&docx.bytes, "word/header1.xml");
    let notes_xml = part(&docx.bytes, "word/footnotes.xml");
    let body_xml = part(&docx.bytes, "word/document.xml");
    for (name, xml, deleted, inserted) in [
        ("header", &header_xml, "draft", "final"),
        ("footnotes", &notes_xml, "thirty", "sixty"),
    ] {
        assert!(
            xml.contains("<w:ins "),
            "{name} carries the insertion: {xml}"
        );
        assert!(
            xml.contains("<w:del "),
            "{name} carries the deletion: {xml}"
        );
        assert!(
            xml.contains(&format!("w:author=\"{AUTHOR}\"")),
            "{name}: {xml}"
        );
        assert!(
            xml.contains(&format!(">{deleted}</w:delText>")),
            "{name}: {xml}"
        );
        assert!(xml.contains(inserted), "{name}: {xml}");
    }
    assert!(
        !body_xml.contains("<w:ins ") && !body_xml.contains("<w:del "),
        "no revision leaked into the body part: {body_xml}"
    );

    // Reopened, the four revisions (an insertion and a deletion in each part)
    // are anchored in the header's and the footnote's own paragraphs, by the
    // session's author.
    let reopened = open(&docx.bytes, Profile::Review);
    let (header_again, header_text) =
        first_paragraph(&reopened, surface_of_kind(&reopened, "header"));
    let (note_again, note_text) =
        first_paragraph(&reopened, surface_of_kind(&reopened, "footnote"));
    assert_eq!(header_text, "Confidential final header");
    assert_eq!(note_text, " The footnote says sixty days.");
    let changes = read::list_tracked_changes(&reopened, None, None).expect("list");
    let mut found: Vec<(String, String)> = changes["trackedChanges"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|change| {
            assert_eq!(change["author"], AUTHOR, "{change:#}");
            let node = change["anchor"]["node"]
                .as_str()
                .expect("an anchor")
                .to_owned();
            let at = if node == header_again {
                "header"
            } else if node == note_again {
                "footnote"
            } else {
                panic!("a revision outside the two parts: {change:#}")
            };
            (
                at.to_owned(),
                change["kind"].as_str().expect("kind").to_owned(),
            )
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("footnote".to_owned(), "deletion".to_owned()),
            ("footnote".to_owned(), "insertion".to_owned()),
            ("header".to_owned(), "deletion".to_owned()),
            ("header".to_owned(), "insertion".to_owned()),
        ]
    );
}
