//! Field spelling, field update attributes, and running-part reachability.
//!
//! Three defects are guarded here, and each guard asserts the **guarantee** rather
//! than the mechanism that currently delivers it:
//!
//! 1. **Every field is written in the complex spelling.** The writer used to emit
//!    an ordinary field as a self-contained `w:fldSimple` and only a legacy form
//!    field as the four-run `w:fldChar` sequence, so a complex field in the source
//!    was normalized down to the simple spelling on save. `docs/128` §5a recorded
//!    that as a real gap; these tests close it.
//!
//! 2. **`w:fldLock` and `w:dirty` survive a round trip.** Both are author intent
//!    about *content*: `fldLock` means Word must not refresh the field, so losing
//!    it lets the reader's Word rewrite the document; `dirty` means the cached
//!    result is stale, so losing it presents a stale value as current. The guards
//!    assert the model state after export → reopen, not that the writer emitted an
//!    attribute — a writer that emitted the attribute and an importer that ignored
//!    it would pass the second and fail the first.
//!
//!    Every one of them carries a **negative** field in the same document, because
//!    the first version of this feature read an absent attribute as *set* (it
//!    reused `is_true`, where a missing `w:val` means `true`) and would have passed
//!    a locked-only assertion while marking every field in every document locked.
//!
//! 3. **A running body no section references is not written.** Toggling Link to
//!    Previous off and on *n* times mints *n* orphaned header bodies, and a writer
//!    that emits every entry of `Definitions::headers` writes all of them on every
//!    save — unbounded growth driven by a checkbox (`docs/128` §3). The acceptance
//!    criterion there is that an unlink/re-link cycle must not leave the package
//!    larger than it started, so that is asserted as a comparison against the
//!    baseline package's own part list, not as a count someone typed in.
//!
//!    The watermark path is guarded beside it, because a reachability pass that
//!    dropped everything unreferenced could have broken it: Word reads a watermark
//!    only from a header, so a header-less section's watermark needs a header part
//!    invented for it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};

use casual_doc_export::{export_document, write_document};
use casual_doc_import::{ImportConfig, import_main_document_xml, import_package};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, FieldUpdateState, FontName, HeaderFooter, HeaderFooterId,
    HeaderFooterKind, HeaderFooterRef, InlineNode, Paragraph, PropChange, Rgba, Run,
    RunProperties, Watermark, WatermarkContent, WatermarkLayout, WatermarkText,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const HEADER_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const FOOTER_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const HEADER_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
const FOOTER_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
const OFFICE_DOC_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

// ---------------------------------------------------------------- helpers

/// Zips the named parts verbatim into a package.
fn zip_named(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in parts {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// `word/document.xml` of a written package, as text.
fn written_document_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

/// Every entry name in a written package.
fn part_names(bytes: &[u8]) -> BTreeSet<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|index| zip.by_index(index).unwrap().name().to_owned())
        .collect()
}

/// A named part's text, if the package contains it.
fn part_text(bytes: &[u8], name: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name(name).ok()?.read_to_string(&mut text).unwrap();
    Some(text)
}

/// Imports a `w:document` body, writes it, and imports the result: the shape every
/// round-trip guarantee below is stated over.
fn round_trip(xml: &[u8]) -> (Document, Document) {
    let first = import_main_document_xml(xml, ImportConfig::default())
        .unwrap()
        .document;
    let bytes = write_document(&first, &BTreeMap::new()).unwrap();
    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    let second = import_package(&mut package, ImportConfig::default())
        .unwrap()
        .document;
    (first, second)
}

/// The update state of every inline field in the body, in document order.
fn field_updates(document: &Document) -> Vec<FieldUpdateState> {
    let mut found = Vec::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::Field(field) = inline {
                    found.push(field.update);
                }
            }
        }
    }
    found
}

/// A single-section package referencing one header part and one footer part, each
/// with recognizable content. Built by hand and imported so the model's
/// `HeaderFooterId`s and the section's references are the importer's own rather
/// than ones this test invented.
fn package_with_one_header_and_footer() -> Vec<u8> {
    let content_types = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="{HEADER_CT}"/><Override PartName="/word/footer1.xml" ContentType="{FOOTER_CT}"/></Types>"#
    );
    let root_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{OFFICE_DOC_REL}" Target="word/document.xml"/></Relationships>"#
    );
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH1"/><w:footerReference w:type="default" r:id="rIdF1"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr></w:body></w:document>"#;
    let doc_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH1" Type="{HEADER_REL}" Target="header1.xml"/><Relationship Id="rIdF1" Type="{FOOTER_REL}" Target="footer1.xml"/></Relationships>"#
    );
    let header = br#"<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>the live header</w:t></w:r></w:p></w:hdr>"#;
    let footer = br#"<w:ftr xmlns:w="urn:w"><w:p><w:r><w:t>the live footer</w:t></w:r></w:p></w:ftr>"#;
    zip_named(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/header1.xml", header),
        ("word/footer1.xml", footer),
    ])
}

/// Imports a package and hands back the model.
fn reopen(bytes: &[u8]) -> Document {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).unwrap();
    import_package(&mut package, ImportConfig::default())
        .unwrap()
        .document
}

/// A recognizable one-paragraph running body, with ids drawn from `seed` so two
/// orphans cannot collide.
fn running_body(seed: u32, text: &str) -> HeaderFooter {
    HeaderFooter {
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(700 + u64::from(seed), 1).unwrap(),
            properties: Default::default(),
            inlines: vec![InlineNode::Run(Run {
                id: NodeId::from_parts(700 + u64::from(seed), 2).unwrap(),
                properties: RunProperties::default().into(),
                text: text.to_owned(),
            })],
        })],
    }
}

/// Mints `count` orphaned header bodies: exactly the state `n` Link-to-Previous
/// off/on cycles leave behind (`CreateHeaderFooterBody` then
/// `SetSectionRunningRef { reference: None }`, with no section left pointing at the
/// body).
fn add_orphan_headers(document: &mut Document, count: u32) -> Vec<HeaderFooterId> {
    let mut ids = Vec::new();
    for seed in 0..count {
        let id = HeaderFooterId::new(NodeId::from_parts(900 + u64::from(seed), 1).unwrap());
        document
            .definitions_mut()
            .headers
            .insert(id, running_body(seed, "an orphaned header"));
        ids.push(id);
    }
    ids
}

fn text_watermark(text: &str) -> Watermark {
    Watermark {
        content: WatermarkContent::Text(WatermarkText {
            text: text.to_owned(),
            font: Some(FontName {
                name: "Calibri".to_owned(),
            }),
            size_half_points: None,
            color: Rgba {
                r: 0xC0,
                g: 0xC0,
                b: 0xC0,
                a: 255,
            },
            bold: false,
            italic: false,
        }),
        layout: WatermarkLayout::Diagonal,
        semi_transparent: true,
    }
}

// ------------------------------------------- 1. one field spelling, the complex one

#[test]
fn a_simple_field_source_is_rewritten_in_the_complex_spelling() {
    // `w:fldSimple` in, the four-run `w:fldChar` sequence out. The guarantee is
    // that the field is still the same field — same instruction, same cached
    // result, reachable by Word's "Update Field" — so the model is compared across
    // the round trip as well as the markup being inspected.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p>
        <w:fldSimple w:instr=" PAGE \* MERGEFORMAT "><w:r><w:t>7</w:t></w:r></w:fldSimple>
    </w:p></w:body></w:document>"#;
    let (first, second) = round_trip(xml);
    assert_eq!(
        first, second,
        "the field survives the change of spelling unchanged"
    );

    let bytes = write_document(&first, &BTreeMap::new()).unwrap();
    let markup = written_document_xml(&bytes);
    assert!(
        !markup.contains("<w:fldSimple"),
        "no field is written as a w:fldSimple any more: {markup}"
    );
    let begin = markup
        .find(r#"<w:fldChar w:fldCharType="begin"/>"#)
        .expect("a fldChar begin run");
    let instr = markup
        .find(r#"<w:instrText xml:space="preserve"> PAGE \* MERGEFORMAT </w:instrText>"#)
        .expect("the instruction, verbatim, as instrText");
    let separate = markup
        .find(r#"<w:fldChar w:fldCharType="separate"/>"#)
        .expect("a fldChar separate run");
    let end = markup
        .find(r#"<w:fldChar w:fldCharType="end"/>"#)
        .expect("a fldChar end run");
    assert!(
        begin < instr && instr < separate && separate < end,
        "begin -> instrText -> separate -> result -> end: {markup}"
    );
    assert!(
        markup[separate..end].contains("<w:t>7</w:t>")
            || markup[separate..end].contains(r#"<w:t xml:space="preserve">7</w:t>"#),
        "the cached result sits between separate and end: {}",
        &markup[separate..end]
    );
}

#[test]
fn a_legacy_form_field_still_carries_its_ff_data_in_the_begin_marker() {
    // The form field was already written in the complex spelling, and sharing one
    // writer with the ordinary field must not have cost it the `w:ffData` block —
    // which is only legal inside a `fldChar begin`, so losing it would make the
    // form control disappear while the field stayed.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p>
        <w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Agree"/><w:checkBox><w:default w:val="1"/><w:checked w:val="1"/></w:checkBox></w:ffData></w:fldChar></w:r>
        <w:r><w:instrText xml:space="preserve"> FORMCHECKBOX </w:instrText></w:r>
        <w:r><w:fldChar w:fldCharType="separate"/></w:r>
        <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:p></w:body></w:document>"#;
    let (first, second) = round_trip(xml);
    assert_eq!(first, second, "the form field survives write -> reopen");

    let bytes = write_document(&first, &BTreeMap::new()).unwrap();
    let markup = written_document_xml(&bytes);
    let begin = markup
        .find(r#"<w:fldChar w:fldCharType="begin">"#)
        .expect("a non-empty fldChar begin, because it carries ffData");
    let ff = markup.find("<w:ffData>").expect("the ffData block");
    let close = markup.find("</w:fldChar>").expect("the begin marker closes");
    assert!(
        begin < ff && ff < close,
        "the ffData block is INSIDE the begin marker: {markup}"
    );
}

// --------------------------------------- 2. w:fldLock and w:dirty round-trip

#[test]
fn a_locked_field_is_still_locked_after_a_round_trip() {
    // Three fields in one paragraph: a locked simple field, a dirty complex field,
    // and one that declares neither. The third is what makes this guard able to
    // fail in both directions — a writer that stamped every field locked, or an
    // importer that read an absent attribute as set (the bug this feature shipped
    // with first), would pass the first two assertions and fail the third.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p>
        <w:fldSimple w:instr=" DATE " w:fldLock="true"><w:r><w:t>1 Jan 2020</w:t></w:r></w:fldSimple>
        <w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r>
        <w:r><w:instrText xml:space="preserve"> NUMPAGES </w:instrText></w:r>
        <w:r><w:fldChar w:fldCharType="separate"/></w:r>
        <w:r><w:t>3</w:t></w:r>
        <w:r><w:fldChar w:fldCharType="end"/></w:r>
        <w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>
    </w:p></w:body></w:document>"#;

    let expected = vec![
        FieldUpdateState {
            locked: true,
            dirty: false,
        },
        FieldUpdateState {
            locked: false,
            dirty: true,
        },
        FieldUpdateState::default(),
    ];

    let (first, second) = round_trip(xml);
    assert_eq!(
        field_updates(&first),
        expected,
        "the import reads each field's own update attributes"
    );
    assert_eq!(
        field_updates(&second),
        expected,
        "and a locked field is still locked, a dirty field still dirty, and a \
         plain field still plain, after export -> reopen"
    );

    // The package side, so a failure says whether the writer or the reader lost it.
    let markup = written_document_xml(&write_document(&first, &BTreeMap::new()).unwrap());
    assert_eq!(
        markup.matches(r#"w:fldLock="true""#).count(),
        1,
        "exactly the one locked field is written locked: {markup}"
    );
    assert_eq!(
        markup.matches(r#"w:dirty="true""#).count(),
        1,
        "exactly the one dirty field is written dirty: {markup}"
    );
}

#[test]
fn an_update_attribute_on_a_later_marker_is_not_lost() {
    // `CT_FldChar` admits `w:fldLock`/`w:dirty` on ANY of a complex field's
    // markers, not only the `begin` Word writes them on. A reader that looked only
    // at `begin` would silently unlock this field.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p>
        <w:r><w:fldChar w:fldCharType="begin"/></w:r>
        <w:r><w:instrText xml:space="preserve"> REF _Ref1 \h </w:instrText></w:r>
        <w:r><w:fldChar w:fldCharType="separate" w:fldLock="true"/></w:r>
        <w:r><w:t>x</w:t></w:r>
        <w:r><w:fldChar w:fldCharType="end" w:dirty="true"/></w:r>
    </w:p></w:body></w:document>"#;
    let (first, second) = round_trip(xml);
    let both = FieldUpdateState {
        locked: true,
        dirty: true,
    };
    assert_eq!(
        field_updates(&first),
        vec![both],
        "flags on the separate and end markers are merged into the field"
    );
    assert_eq!(
        field_updates(&second),
        vec![both],
        "and both survive export -> reopen (we write them on begin, where Word does)"
    );
}

#[test]
fn a_paragraph_spanning_fields_lock_survives_the_round_trip() {
    // The range encoding carries the same two flags, in the definitions table
    // rather than on an inline node. A field that outgrew its paragraph must not
    // lose its lock by changing shape.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin" w:fldLock="true"/></w:r>
            <w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
        </w:p>
        <w:p><w:r><w:t>First chapter</w:t></w:r></w:p>
        <w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
    </w:body></w:document>"#;
    let (first, second) = round_trip(xml);
    for (label, document) in [("import", &first), ("reopen", &second)] {
        let ranges: Vec<_> = document
            .definitions()
            .field_ranges
            .iter()
            .map(|(_, range)| range.update)
            .collect();
        assert_eq!(
            ranges,
            vec![FieldUpdateState {
                locked: true,
                dirty: false,
            }],
            "the paragraph-spanning field is locked after {label}"
        );
    }
}

// ------------------------------- 3. a running body nobody references is not written

#[test]
fn an_unreferenced_running_body_is_not_written_and_the_drop_is_reported() {
    // Unlink then re-link leaves a body with no section pointing at it. Writing it
    // is what made a checkbox grow the file without bound.
    let baseline = package_with_one_header_and_footer();
    let clean = reopen(&baseline);
    let clean_bytes = write_document(&clean, &BTreeMap::new()).unwrap();

    let mut orphaned = reopen(&baseline);
    add_orphan_headers(&mut orphaned, 3);
    assert_eq!(
        orphaned.definitions().headers.len(),
        4,
        "the model holds the live header plus three orphans"
    );
    let export = export_document(&orphaned, &BTreeMap::new()).unwrap();

    assert_eq!(
        part_names(&export.bytes),
        part_names(&clean_bytes),
        "three unlink/re-link cycles leave the package's part list unchanged — the \
         acceptance criterion docs/128 §3 states"
    );
    assert!(
        export.bytes.len() <= clean_bytes.len(),
        "and the package is no larger: {} vs {}",
        export.bytes.len(),
        clean_bytes.len()
    );
    assert_eq!(
        part_text(&export.bytes, "word/header1.xml").as_deref(),
        part_text(&clean_bytes, "word/header1.xml").as_deref(),
        "the header a section DOES reference is written, unchanged"
    );
    assert!(
        part_text(&export.bytes, "word/header1.xml")
            .is_some_and(|text| text.contains("the live header")),
        "and it is the live header, not an orphan that took its part name"
    );

    let reported: Vec<&str> = export
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(
        reported.contains(&"docx.export.header.unreferenced_dropped"),
        "the drop is NAMED, not swallowed: {reported:?}"
    );
    assert!(
        !reported.contains(&"docx.export.footer.unreferenced_dropped"),
        "and nothing is claimed about footers, which have no orphan here: {reported:?}"
    );
}

#[test]
fn an_unreferenced_footer_body_is_dropped_too() {
    // The same rule for the other region, because two loops with one rule is how
    // one of them comes to be forgotten.
    let baseline = package_with_one_header_and_footer();
    let clean_bytes = write_document(&reopen(&baseline), &BTreeMap::new()).unwrap();

    let mut orphaned = reopen(&baseline);
    let id = HeaderFooterId::new(NodeId::from_parts(950, 1).unwrap());
    orphaned
        .definitions_mut()
        .footers
        .insert(id, running_body(50, "an orphaned footer"));
    let export = export_document(&orphaned, &BTreeMap::new()).unwrap();

    assert_eq!(part_names(&export.bytes), part_names(&clean_bytes));
    assert!(
        part_text(&export.bytes, "word/footer1.xml")
            .is_some_and(|text| text.contains("the live footer")),
        "the referenced footer keeps footer1.xml"
    );
    assert!(
        export
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "docx.export.footer.unreferenced_dropped"),
        "the footer drop is reported"
    );
}

#[test]
fn a_running_body_reached_only_from_a_sect_pr_change_snapshot_is_still_written() {
    // A `w:sectPrChange`'s prior snapshot carries its own `w:headerReference`, and
    // the writer emits it. A reachability pass that looked only at the live section
    // would leave that reference naming a part the package does not contain — a
    // broken package, which is exactly the invariant FID-R-06 exists for.
    let mut document = reopen(&package_with_one_header_and_footer());
    let historic = HeaderFooterId::new(NodeId::from_parts(960, 1).unwrap());
    document
        .definitions_mut()
        .headers
        .insert(historic, running_body(60, "the header before the change"));
    let section = &mut document.definitions_mut().sections[0];
    let mut prior = section.clone();
    prior.section_change = None;
    prior.headers = vec![HeaderFooterRef {
        kind: HeaderFooterKind::Default,
        reference: historic,
    }];
    section.section_change = Some(PropChange {
        author: Some("alice".to_owned()),
        date: None,
        revision_id: None,
        editor_group: None,
        prior: Box::new(prior),
    });

    let export = export_document(&document, &BTreeMap::new()).unwrap();
    let names = part_names(&export.bytes);
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with("word/header"))
            .count(),
        2,
        "both the live header and the one the revision record names are written: {names:?}"
    );
    let bodies: Vec<String> = ["word/header1.xml", "word/header2.xml"]
        .iter()
        .filter_map(|name| part_text(&export.bytes, name))
        .collect();
    assert!(
        bodies
            .iter()
            .any(|text| text.contains("the header before the change")),
        "the prior snapshot's header body is one of them: {bodies:?}"
    );
    assert!(
        !export
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "docx.export.header.unreferenced_dropped"),
        "and nothing is reported dropped, because nothing was"
    );

    // Every `r:id` a `w:headerReference` names must exist in the part's own
    // relationships: the guarantee, rather than a part count.
    let markup = written_document_xml(&export.bytes);
    let rels = part_text(&export.bytes, "word/_rels/document.xml.rels").unwrap();
    let mut references = 0;
    for (index, _) in markup.match_indices("<w:headerReference") {
        let element = &markup[index..index + markup[index..].find("/>").unwrap()];
        let at = element.find("r:id=\"").expect("a headerReference has an r:id");
        let rest = &element[at + 6..];
        let rel_id = &rest[..rest.find('"').unwrap()];
        assert!(
            rels.contains(&format!(r#"Id="{rel_id}""#)),
            "the {rel_id} relationship exists for {element}: {rels}"
        );
        references += 1;
    }
    assert_eq!(references, 2, "two header references were written: {markup}");
}

#[test]
fn a_header_less_sections_watermark_still_gets_its_part_beside_an_orphan() {
    // The one case a reachability pass could have broken. Word reads a watermark
    // only from a header, so a section with no header of a page type it uses needs
    // a header part invented for it — and that part is NOT an entry of
    // `Definitions::headers`, so the pass must not be able to see it, let alone
    // drop it. An orphan sits in the same document to prove the pass ran.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>body</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr></w:body></w:document>"#;
    let mut document = import_main_document_xml(xml, ImportConfig::default())
        .unwrap()
        .document;
    document.definitions_mut().sections[0].watermark = Some(text_watermark("DRAFT"));
    add_orphan_headers(&mut document, 2);

    let export = export_document(&document, &BTreeMap::new()).unwrap();
    let names = part_names(&export.bytes);
    let headers: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("word/header"))
        .collect();
    assert_eq!(
        headers.len(),
        1,
        "exactly one header part: the synthesized watermark carrier, and neither \
         orphan: {names:?}"
    );
    let carrier = part_text(&export.bytes, headers[0]).unwrap();
    assert!(
        carrier.contains("DRAFT") && carrier.contains("v:textpath"),
        "it carries the watermark shape: {carrier}"
    );
    assert!(
        !carrier.contains("an orphaned header"),
        "and it is not an orphan's content: {carrier}"
    );

    let markup = written_document_xml(&export.bytes);
    assert!(
        markup.contains(r#"<w:headerReference w:type="default" r:id="rIdWm"#),
        "and the section references it, or Word never finds the stamp: {markup}"
    );
}

#[test]
fn a_watermark_in_a_referenced_header_survives_beside_an_orphan() {
    // The other half: when the section DOES have a header, the watermark rides in
    // that part. `watermark_plan` keys `in_header` off the section's own
    // references, so a watermark-carrying part is referenced by construction — this
    // asserts that rather than assuming it.
    let mut document = reopen(&package_with_one_header_and_footer());
    document.definitions_mut().sections[0].watermark = Some(text_watermark("CONFIDENTIAL"));
    add_orphan_headers(&mut document, 1);

    let export = export_document(&document, &BTreeMap::new()).unwrap();
    let header = part_text(&export.bytes, "word/header1.xml").expect("the live header is written");
    assert!(
        header.contains("CONFIDENTIAL") && header.contains("the live header"),
        "the stamp is in the header the section references, alongside its content: {header}"
    );
    assert_eq!(
        part_names(&export.bytes)
            .iter()
            .filter(|name| name.starts_with("word/header"))
            .count(),
        1,
        "and the orphan did not become a second header part"
    );
}

#[test]
fn a_document_with_no_orphans_reports_nothing_about_running_parts() {
    // The report has to stay worth reading: a finding that fires on healthy
    // documents is one every caller learns to ignore.
    let export = export_document(
        &reopen(&package_with_one_header_and_footer()),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        export.report.is_empty(),
        "a clean document exports with an empty report: {:?}",
        export.report
    );
}
