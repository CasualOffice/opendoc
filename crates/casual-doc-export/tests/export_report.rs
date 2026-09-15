//! Export-side loss reporting (FID-R-01) and the package invariant it rests on
//! (FID-R-06).
//!
//! Two properties are asserted here, and they pull against each other on
//! purpose:
//!
//! - a document the writer emits in full reports **nothing**, so the report
//!   stays worth reading;
//! - content the package cannot carry is reported **and** left out of the
//!   package cleanly, so no relationship names a part that is absent or empty.
//!
//! The second is what makes the first safe to rely on: before this, a missing
//! image produced a zero-byte part with a live `/image` relationship and an
//! empty report, so a silently broken file was indistinguishable from a clean
//! save.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};

use casual_doc_export::{export_document, export_document_with_retained_parts};
use casual_doc_import::{
    ImportConfig, ImportMode, RetainedPart, RetainedParts, import_main_document_xml, import_package,
};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Document, Extent, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use quick_xml::Reader;
use quick_xml::events::Event;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const RICH: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");

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

/// Every part of a package, keyed by name.
fn parts_of(package: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(package.to_vec())).unwrap();
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        let mut bytes = Vec::new();
        archive
            .by_name(&name)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        parts.insert(name, bytes);
    }
    parts
}

/// Imports a package semantically and hands back the model plus the binary parts
/// a caller would carry alongside it, exactly as the format adapter does: image
/// media and embedded `.odttf` faces, keyed by part name.
fn import(package: &[u8]) -> (Document, BTreeMap<String, Vec<u8>>) {
    let mut opened = DocxPackage::open(package, PackageLimits::default()).unwrap();
    let imported = import_package(
        &mut opened,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap();
    let mut resources = BTreeMap::new();
    for (_, reference) in imported.document.definitions().media.iter() {
        if let Ok(bytes) = opened.read_part(&reference.part_name) {
            resources.insert(reference.part_name.clone(), bytes);
        }
    }
    for font in &imported.document.definitions().font_table {
        for (_, face) in font.embedded.faces() {
            if let Ok(bytes) = opened.read_part(&face.part_name) {
                resources.insert(face.part_name.clone(), bytes);
            }
        }
    }
    (imported.document, resources)
}

/// The relationship ids declared by one `.rels` part, and the internal targets
/// they resolve to (external targets are not package parts and are skipped).
fn internal_targets(rels_part_name: &str, bytes: &[u8]) -> Vec<(String, String)> {
    // `word/_rels/document.xml.rels` describes `word/document.xml`, so a relative
    // target resolves against `word/`.
    let base = rels_part_name
        .rsplit_once("_rels/")
        .map(|(prefix, _)| prefix.to_owned())
        .unwrap_or_default();
    let mut reader = Reader::from_reader(bytes);
    let mut targets = Vec::new();
    let mut buffer = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .expect("well-formed rels");
        let element = match &event {
            Event::Start(element) | Event::Empty(element) => element,
            Event::Eof => break,
            _ => {
                buffer.clear();
                continue;
            }
        };
        if element.local_name().as_ref() != b"Relationship" {
            buffer.clear();
            continue;
        }
        let mut id = String::new();
        let mut target = String::new();
        let mut external = false;
        for attribute in element.attributes() {
            let attribute = attribute.expect("well-formed attribute");
            let value = String::from_utf8_lossy(&attribute.value).into_owned();
            match attribute.key.local_name().as_ref() {
                b"Id" => id = value,
                b"Target" => target = value,
                b"TargetMode" => external = value == "External",
                _ => {}
            }
        }
        if !external {
            let resolved = match target.strip_prefix('/') {
                Some(absolute) => absolute.to_owned(),
                None => normalize(&format!("{base}{target}")),
            };
            targets.push((id, resolved));
        }
        buffer.clear();
    }
    targets
}

/// Collapses `.` and `..` segments in a resolved part name, so a relationship
/// written as `../customXml/item1.xml` from inside `word/` is compared against
/// the name the package actually uses.
fn normalize(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// Asserts the package-level invariant: no relationship names a part the package
/// does not contain, and no part it does contain is empty.
///
/// A zero-byte part behind a live relationship is a package that advertises
/// content it cannot supply; a relationship with no part at all is worse. Both
/// are indistinguishable from a healthy file until Word opens it, which is why
/// this is checked structurally rather than eyeballed per feature.
fn assert_package_is_self_consistent(package: &[u8], label: &str) {
    let parts = parts_of(package);
    for (name, bytes) in &parts {
        assert!(
            !bytes.is_empty(),
            "{label}: part {name} is zero-byte; a declared part must carry content"
        );
    }
    for (name, bytes) in &parts {
        if !name.contains("_rels/") {
            continue;
        }
        for (id, target) in internal_targets(name, bytes) {
            assert!(
                parts.contains_key(&target),
                "{label}: {name} declares {id} -> {target}, which the package does not contain"
            );
        }
    }
}

/// The features a report names, for readable assertions.
fn features(report: &casual_doc_export::CompatibilityReport) -> BTreeSet<&str> {
    report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect()
}

/// A real Word document, exported with the bytes it came with, must report
/// nothing at all.
///
/// This is the guard against the opposite failure from FID-R-01: a report that
/// fires on healthy documents is one every caller learns to ignore, and an
/// ignored report is worse than none. The fixture is an ordinary Word file with
/// a picture, styles, a font table, settings and metadata — every one of which
/// the writer emits in full.
#[test]
fn an_ordinary_document_exports_with_an_empty_report() {
    let (document, resources) = import(RICH);
    assert_eq!(
        document.definitions().media.iter().count(),
        1,
        "the fixture must exercise the media path, or the empty report proves little"
    );
    let export = export_document(&document, &resources).expect("the fixture exports");
    assert!(
        export.report.is_empty(),
        "an ordinary document must report no loss, got {:?}",
        export.report.entries
    );
    assert_package_is_self_consistent(&export.bytes, "rich fixture");
}

/// A media reference whose bytes the caller does not supply is reported, and the
/// part, its relationship and the body reference are dropped together.
#[test]
fn a_media_reference_without_bytes_is_reported_and_left_out_whole() {
    let (document, _) = import(RICH);
    let part_name = document
        .definitions()
        .media
        .iter()
        .map(|(_, reference)| reference.part_name.clone())
        .next()
        .expect("the fixture has one picture");

    // Exported with NO media bytes: the model still declares the picture.
    let export = export_document(&document, &BTreeMap::new()).expect("the document still exports");

    let entry = export
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "docx.export.media.missing_bytes")
        .expect("a media part with no bytes must be reported");
    assert_eq!(entry.part_name.as_deref(), Some(part_name.as_str()));
    assert_eq!(
        entry.model_outcome(),
        casual_doc_export::ModelOutcome::Omitted
    );
    assert_eq!(
        entry.retention_outcome(),
        casual_doc_export::RetentionOutcome::NotRetained
    );

    let parts = parts_of(&export.bytes);
    assert!(
        !parts.contains_key(&part_name),
        "the media part must not be written at all, let alone empty"
    );
    let rels = String::from_utf8(
        parts
            .get("word/_rels/document.xml.rels")
            .expect("document rels")
            .clone(),
    )
    .unwrap();
    assert!(
        !rels.contains("/image"),
        "no /image relationship may survive the part it names: {rels}"
    );
    let body = String::from_utf8(parts.get("word/document.xml").expect("body").clone()).unwrap();
    assert!(
        !body.contains("<w:drawing>"),
        "the drawing must be dropped rather than point at nothing"
    );
    assert_package_is_self_consistent(&export.bytes, "rich fixture without media bytes");
}

/// A package embedding one `.odttf` face, present or absent from the archive.
fn package_with_embedded_face(include_face: bool) -> Vec<u8> {
    let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#;
    let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#;
    let font_table = br#"<w:fonts xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:font w:name="Calibri"><w:embedRegular r:id="rIdF1" w:fontKey="{6C99A02D-4E1B-4E5A-9F0C-1234567890AB}"/></w:font></w:fonts>"#;
    let font_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdF1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font1.odttf"/></Relationships>"#;
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", root_rels),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", doc_rels),
        ("word/fontTable.xml", font_table),
        ("word/_rels/fontTable.xml.rels", font_rels),
    ];
    if include_face {
        parts.push(("word/fonts/font1.odttf", b"ODTTF-OBFUSCATED-FACE-BYTES"));
    }
    zip_named(&parts)
}

/// An embedded face whose `.odttf` bytes are missing is reported, and the
/// `w:embedRegular`, the `/font` relationship and the part are dropped together.
///
/// PR #534 fixed the import half (the bytes are now carried out of the package);
/// this is the export half — with no bytes to write, the writer used to announce
/// the face anyway and put a zero-byte `.odttf` behind it.
#[test]
fn an_embedded_face_without_bytes_is_reported_and_left_out_whole() {
    let (document, resources) = import(&package_with_embedded_face(false));
    assert!(
        resources.is_empty(),
        "the face is declared but absent, so nothing is carried"
    );
    assert!(
        document.definitions().font_table[0]
            .embedded
            .regular
            .is_some(),
        "the model keeps the face reference"
    );

    let export = export_document(&document, &resources).expect("the document still exports");
    let entry = export
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "docx.export.embedded_font.missing_bytes")
        .expect("an embedded face with no bytes must be reported");
    assert_eq!(entry.part_name.as_deref(), Some("word/fonts/font1.odttf"));

    let parts = parts_of(&export.bytes);
    assert!(!parts.contains_key("word/fonts/font1.odttf"));
    assert!(
        !parts.contains_key("word/_rels/fontTable.xml.rels"),
        "with no face left there is no /font relationship part to write"
    );
    let font_table =
        String::from_utf8(parts.get("word/fontTable.xml").expect("font table").clone()).unwrap();
    assert!(
        !font_table.contains("w:embedRegular"),
        "the face must not be announced without bytes behind it: {font_table}"
    );
    assert_package_is_self_consistent(&export.bytes, "embedded face without bytes");
}

/// The same package WITH the face bytes must report nothing and still embed it.
#[test]
fn an_embedded_face_with_bytes_reports_nothing() {
    let (document, resources) = import(&package_with_embedded_face(true));
    let export = export_document(&document, &resources).expect("the document exports");
    assert!(
        export.report.is_empty(),
        "a face whose bytes are present round-trips and must raise nothing, got {:?}",
        export.report.entries
    );
    let parts = parts_of(&export.bytes);
    assert_eq!(
        parts.get("word/fonts/font1.odttf").map(Vec::as_slice),
        Some(b"ODTTF-OBFUSCATED-FACE-BYTES".as_slice())
    );
    assert_package_is_self_consistent(&export.bytes, "embedded face with bytes");
}

/// `w:background` is imported into the model and has no writer, so every save
/// drops the page fill. Emitting it is FID-R-04's fidelity fix; until then the
/// loss must at least be named, exactly as the ODT writer names its own.
#[test]
fn a_page_background_the_writer_cannot_emit_is_reported() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:background w:color="FFF9ED"/><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let document = import_main_document_xml(xml, ImportConfig::default())
        .unwrap()
        .document;
    assert!(
        document.background().is_some(),
        "the importer captures the page fill"
    );
    let export = export_document(&document, &BTreeMap::new()).expect("exports");
    assert!(features(&export.report).contains("docx.export.background"));

    // And a document without one must not raise it.
    let plain = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let plain = import_main_document_xml(plain, ImportConfig::default())
        .unwrap()
        .document;
    let export = export_document(&plain, &BTreeMap::new()).expect("exports");
    assert!(
        export.report.is_empty(),
        "a document with no page fill must raise nothing, got {:?}",
        export.report.entries
    );
}

/// A document whose body references an embedded chart part.
fn document_with_chart() -> Document {
    use casual_doc_model::v1::{
        BlockNode, Definitions, EmbeddedKind, EmbeddedObject, EmbeddedPart, Paragraph,
    };

    let node = |counter| NodeId::from_parts(1, counter).unwrap();
    let object = InlineNode::EmbeddedObject(EmbeddedObject {
        id: node(3),
        kind: EmbeddedKind::Chart,
        part: EmbeddedPart {
            relationship_id: "rIdChart1".to_owned(),
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart"
                    .to_owned(),
            part_name: "word/charts/chart1.xml".to_owned(),
        },
        extra_parts: Vec::new(),
        preview: None,
        extent: Extent {
            width_emu: 914_400,
            height_emu: 685_800,
        },
        prog_id: None,
    });
    let body = vec![BlockNode::Paragraph(Paragraph {
        id: node(2),
        properties: Default::default(),
        inlines: vec![object],
    })];
    Document::new(node(1), body, Definitions::default()).unwrap()
}

/// A chart the package does not contain is reported.
///
/// An embedded object's bytes come only from the opaque side-table, so a
/// semantic export without retention emits the body reference and the
/// relationship while the part itself is absent — a live relationship pointing
/// at nothing. That is deliberately *reported* rather than repaired here:
/// repairing it means deciding what the body says instead of the chart, which is
/// FID-R-08's open scope question. Reporting is what stops it being invisible.
#[test]
fn an_embedded_object_part_the_package_lacks_is_reported() {
    let document = document_with_chart();
    let export = export_document(&document, &BTreeMap::new()).expect("exports");
    let entry = export
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "docx.export.embedded_object.missing_part")
        .expect("a chart part the package lacks must be reported");
    assert_eq!(entry.part_name.as_deref(), Some("word/charts/chart1.xml"));
}

/// The same chart, with the part carried through the side-table, reports
/// nothing: it genuinely round-trips.
#[test]
fn an_embedded_object_part_carried_verbatim_reports_nothing() {
    let document = document_with_chart();
    let retained = RetainedParts {
        parts: vec![RetainedPart {
            part_name: "word/charts/chart1.xml".to_owned(),
            content_type: Some(
                "application/vnd.openxmlformats-officedocument.drawingml.chart+xml".to_owned(),
            ),
            bytes: b"<c:chartSpace/>".to_vec(),
            rels: None,
        }],
        relationships: Vec::new(),
    };
    let export = export_document_with_retained_parts(&document, &BTreeMap::new(), &retained)
        .expect("exports");
    assert!(
        export.report.is_empty(),
        "a retained chart part must raise nothing, got {:?}",
        export.report.entries
    );
    assert_package_is_self_consistent(&export.bytes, "chart carried verbatim");
}
